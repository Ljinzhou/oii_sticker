//! 应用自动更新：镜像探测择优 + 官方 updater 插件编排。
//!
//! 职责边界：本模块只负责「选路、重试、状态上报」；下载、minisign 验签与
//! NSIS 静默安装全部委托 tauri-plugin-updater（安全敏感逻辑不自己实现）。
//!
//! 选路机制：CI 为每个加速镜像发布一份 latest 变体清单（归档字节与签名完全一致，
//! 仅资产 URL 前缀不同）。运行时并发探测各镜像延迟 → 重排变体清单 URL 交给
//! UpdaterBuilder::endpoints；插件对非 2XX 自动落到下一个端点的原生语义即成为
//! 镜像故障切换链。每次重试轮换主选镜像，重建出的 Update 其 download_url 已带
//! 对应镜像前缀，因此下载流量同样走择优后的镜像。
//!
//! 对前端暴露三个命令：update_check_cmd / update_download_cmd / update_state_cmd，
//! 以及两个事件 updater://progress、updater://phase。

use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use serde::Serialize;
use tauri::{AppHandle, Emitter, Url};
use tauri_plugin_updater::UpdaterExt;

/// 仓库发布基址（直连）。
const RELEASE_BASE: &str = "https://github.com/Ljinzhou/oii_sticker/releases/latest/download";
/// 手动兜底外链（所有致命错误的最终出路）。
pub const MANUAL_RELEASES_URL: &str = "https://github.com/Ljinzhou/oii_sticker/releases";

/// 加速镜像前缀（"" = 直连 GitHub）。顺序仅是探测前的初始偏好，实际以延迟排序为准。
/// 默认公网代理命中失败/需内网镜像时，可用环境变量 `OII_STICKER_UPDATE_MIRRORS`
/// 追加（逗号分隔的 URL 前缀，追加在直连之后、内置代理之前）。
pub const MIRROR_PREFIXES: &[&str] = &[
    "",
    "https://gh-proxy.org/",
    "https://v4.gh-proxy.org/",
    "https://v6.gh-proxy.org/",
    "https://cdn.gh-proxy.org/",
    "https://axisnow.gh-proxy.org/",
];

/// 完整镜像前缀表（内置 + 环境变量追加），探测/构造 URL 统一走这里。
fn mirror_prefixes() -> Vec<String> {
    let extra = std::env::var("OII_STICKER_UPDATE_MIRRORS").unwrap_or_default();
    extend_prefixes(MIRROR_PREFIXES, &extra)
}

/// 内置前缀 + 环境变量前缀合并（纯函数，便于单测）：
/// 直连永远第一 → 环境变量前缀 → 内置代理。
fn extend_prefixes(base: &[&str], extra: &str) -> Vec<String> {
    let extras: Vec<String> = extra
        .split(',')
        .map(|s| s.trim().trim_end_matches('/'))
        .filter(|s| !s.is_empty())
        .map(|s| format!("{s}/"))
        .collect();
    base.iter().map(|s| s.to_string()).chain(extras).collect()
}

/// 单次探测超时。
const PROBE_TIMEOUT: Duration = Duration::from_secs(4);
/// 检查阶段（拉取元数据清单）请求超时：应短，坏镜像尽快失效落选。
const CHECK_TIMEOUT: Duration = Duration::from_secs(30);
/// 下载阶段请求超时：大安装包 + 慢网需要宽松，作为请求总时长上限。
const DOWNLOAD_TIMEOUT: Duration = Duration::from_secs(900);
/// 探测结果缓存时长。
const PROBE_CACHE: Duration = Duration::from_secs(600);
/// 传输类失败的自动重试次数（不含首次；每次轮换到次优镜像）。
const MAX_RETRIES: u32 = 2;

// ───────────────────────── 错误模型 ─────────────────────────

/// 归一化错误分类（决定「重试轮换」还是「立即终止」）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum UpdateErrKind {
    /// 网络/DNS/超时/连接重置等传输类问题——可换镜像重试。
    Network,
    /// 清单或资产 404——可轮换下一镜像。
    NotFound,
    /// 验签失败——签名各镜像同源，轮换无意义，直接终止。
    Signature,
    /// 安装阶段失败（安装器退出码/权限）——终止。
    Install,
    /// 其他未知错误——按可重试处理。
    Unknown,
}

/// 归一化错误（前端只渲染，不再解析字符串）。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateError {
    kind: UpdateErrKind,
    message: String,
    manual_url: String,
}

impl UpdateError {
    pub fn new(kind: UpdateErrKind, message: impl Into<String>) -> Self {
        Self { kind, message: message.into(), manual_url: MANUAL_RELEASES_URL.into() }
    }
}

/// 把插件/reqwest 的错误文本归类为结构化错误。
/// 以字符串启发式匹配插件 Error 的 Display 输出；纯函数便于单测。
pub fn classify_error(raw: &str) -> UpdateError {
    let lower = raw.to_lowercase();
    if lower.contains("signature") {
        return UpdateError::new(
            UpdateErrKind::Signature,
            format!("更新包签名校验失败，为防安全问题已中止：{raw}"),
        );
    }
    if lower.contains("404") || lower.contains("not found") || lower.contains("no matching releases") {
        return UpdateError::new(
            UpdateErrKind::NotFound,
            format!("更新服务器上没有找到可用资源：{raw}"),
        );
    }
    if lower.contains("install") || lower.contains("msiexec") || lower.contains("nsis")
        || lower.contains("permission") || lower.contains("access is denied")
    {
        return UpdateError::new(
            UpdateErrKind::Install,
            format!("安装更新失败（可能需要管理员权限）：{raw}"),
        );
    }
    // 磁盘空间不足：写临时安装包失败属「无法继续」，重试无意义，按安装失败呈现。
    if lower.contains("no space") || lower.contains("insufficient") || lower.contains("disk full")
        || lower.contains("not enough space")
    {
        return UpdateError::new(
            UpdateErrKind::Install,
            format!("磁盘空间不足，无法下载/安装更新，请清理后重试：{raw}"),
        );
    }
    if lower.contains("timeout") || lower.contains("timed out")
        || lower.contains("connection") || lower.contains("reset") || lower.contains("refused")
        || lower.contains("dns") || lower.contains("network") || lower.contains("proxy")
        || lower.contains("channel closed")
        || lower.contains("certificate") || lower.contains("ssl") || lower.contains("tls")
    {
        return UpdateError::new(UpdateErrKind::Network, format!("网络异常：{raw}"));
    }
    UpdateError::new(UpdateErrKind::Unknown, raw.to_string())
}

// ───────────────────────── 探测 ─────────────────────────

/// 单镜像探测结果。
///
/// - `ok=true`：拿到了 2xx 响应，镜像确定可用；
/// - `ok=false, responded=true`：拿到了 HTTP 响应但状态码异常（典型：仓库尚无
///   Release 时 latest.json 全线 404）——网络本身是通的；
/// - `responded=false`：DNS/超时/拒连等传输层失败，该镜像本轮视为不存在。
#[derive(Debug, Clone, PartialEq)]
struct ProbeHit {
    endpoint: Url,
    #[allow(dead_code)]
    latency_ms: u128,
    responded: bool,
    ok: bool,
}

/// 构造某镜像下的清单变体 URL。纯函数，单测覆盖。
fn manifest_url(mirror_index: usize) -> Option<Url> {
    let prefixes = mirror_prefixes();
    let prefix = prefixes.get(mirror_index)?;
    Url::parse(&format!("{prefix}{RELEASE_BASE}/latest.json")).ok()
}

/// 探测结果 → 最终端点顺序。规则（用户指定）：
/// 1. 直连 GitHub 永远排第一（本机有代理/网络良好时它就是最优解）；
/// 2. 其余可用镜像按实测延迟升序跟随；
/// 3. 只响应了非 2xx 的端点排在最后（典型场景：仓库还没发布过版本，
///    全线 404——网络是通的，仍交给插件走正式检查以获得准确结论）；
/// 4. 传输层失败的端点剔除。纯函数，单测覆盖。
fn rank_probe_hits(hits: &[ProbeHit]) -> Vec<Url> {
    let mut sorted: Vec<&ProbeHit> = hits.iter().filter(|h| h.responded).collect();
    sorted.sort_by(|a, b| {
        let direct = |h: &ProbeHit| h.endpoint.as_str().starts_with("https://github.com/");
        b.ok.cmp(&a.ok) // 可用优先
            .then_with(|| direct(b).cmp(&direct(a))) // 直连永远最前
            .then_with(|| a.latency_ms.cmp(&b.latency_ms)) // 再按延迟
    });
    sorted.into_iter().map(|h| h.endpoint.clone()).collect()
}

/// 并发探测所有镜像（JoinSet），返回按延迟升序的端点表；全败则空表。
async fn probe_mirrors() -> Vec<ProbeHit> {
    let client = reqwest::Client::builder()
        .user_agent("oii_sticker/update-probe")
        .timeout(PROBE_TIMEOUT)
        .build()
        .expect("probe http client 构建失败（静态配置）");
    let mut jobs = tokio::task::JoinSet::new();
    for i in 0..mirror_prefixes().len() {
        let client = client.clone();
        jobs.spawn(async move {
            let url = manifest_url(i)?;
            let started = Instant::now();
            // 小体积 GET 实测可达性与延迟（部分镜像不支持 HEAD）。
            let mut resp = match client.get(url.clone()).send().await {
                Ok(r) => r,
                Err(_) => return None, // 传输层失败：DNS/超时/拒连
            };
            let responded = true;
            let ok = resp.status().is_success();
            // 2xx 才取首块确认内容可达；非 2xx（典型 404=尚无 Release）保留响应事实即可。
            if ok {
                let _ = resp.chunk().await.ok()?;
            }
            Some(ProbeHit { endpoint: url, latency_ms: started.elapsed().as_millis(), responded, ok })
        });
    }
    let mut hits = Vec::new();
    while let Some(res) = jobs.join_next().await {
        if let Ok(Some(hit)) = res {
            hits.push(hit);
        }
    }
    hits.sort_by_key(|h| h.latency_ms);
    hits
}

// ───────────────────────── 共享状态 ─────────────────────────

/// 更新流程状态机（事件负载 / 前端断线重接快照）。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "snake_case", tag = "phase")]
pub enum UpdatePhase {
    Idle,
    Checking,
    /// 已是最新。
    UpToDate { current: String },
    /// 发现新版本，等待用户确认下载。
    Available { current: String, version: String, notes: Option<String> },
    Downloading { downloaded: u64, total: Option<u64>, retrying: bool },
    Installing,
    /// 安装成功，即将自动重启。
    Restarting,
    Failed { code: UpdateErrKind, message: String, manual_url: String },
}

/// Tauri 托管的全局更新状态。
#[derive(Default)]
pub struct UpdateState {
    inner: Arc<Mutex<StateInner>>,
}

#[derive(Default)]
struct StateInner {
    phase: Option<UpdatePhase>,
    /// 缓存的探测结果（端点表 + 时间戳），PROBE_CACHE 内复用。
    probe_cache: Option<(Vec<Url>, Instant)>,
    /// 已发现的待安装更新的端点表。
    pending_endpoints: Option<Vec<Url>>,
    /// 后台下载任务是否在跑（防重复触发）。
    busy: bool,
}

impl UpdateState {
    fn set_phase(&self, app: &AppHandle, phase: UpdatePhase) {
        if let Ok(mut g) = self.inner.lock() {
            g.phase = Some(phase.clone());
        }
        let _ = app.emit("updater://phase", &phase);
    }

    fn current_phase(&self) -> Option<UpdatePhase> {
        self.inner.lock().ok().and_then(|g| g.phase.clone())
    }

    fn shared(&self) -> Arc<Mutex<StateInner>> {
        self.inner.clone()
    }

    /// 原子地「检查并占用」busy 标志（锁内 check-and-set，防并发双下载）。
    fn try_begin_busy(&self) -> bool {
        self.inner
            .lock()
            .map(|mut g| {
                if g.busy {
                    false
                } else {
                    g.busy = true;
                    true
                }
            })
            .unwrap_or(false)
    }
}

/// shared 形态的当前阶段读取（供异步流程与 on_before_exit 回调使用）。
fn current_phase_shared(state: &Arc<Mutex<StateInner>>) -> Option<UpdatePhase> {
    state.lock().ok().and_then(|g| g.phase.clone())
}

// ───────────────────────── 命令 ─────────────────────────

/// 检查更新命令返回值。
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CheckOutcome {
    pub phase: UpdatePhase,
}

/// 检查更新：探测镜像 → 经插件拉取清单并做语义化版本比较。
/// 不下载安装包；发现新版后进入 Available 状态等待用户确认。
#[tauri::command]
pub async fn update_check_cmd(
    app: AppHandle,
    state: tauri::State<'_, UpdateState>,
) -> Result<CheckOutcome, UpdateError> {
    let shared = state.shared();
    check_flow(&app, &shared).await.map(|phase| CheckOutcome { phase })
}

/// 统一构造插件 Updater：endpoints 故障切换链 + 指定请求超时。
fn build_updater(
    app: &AppHandle,
    endpoints: &[Url],
    timeout: Duration,
) -> Result<tauri_plugin_updater::Updater, UpdateError> {
    let builder = app
        .updater_builder()
        .endpoints(endpoints.to_vec())
        .map_err(|e| classify_error(&e.to_string()))?;
    builder.timeout(timeout).build().map_err(|e| classify_error(&e.to_string()))
}

async fn check_flow(app: &AppHandle, shared: &Arc<Mutex<StateInner>>) -> Result<UpdatePhase, UpdateError> {
    use UpdatePhase as P;
    set_phase_shared(app, shared, P::Checking);

    // 1) 端点表：缓存命中则跳过探测。
    let endpoints = match shared
        .lock()
        .ok()
        .and_then(|g| g.probe_cache.as_ref().and_then(|(urls, at)| {
            (at.elapsed() < PROBE_CACHE).then(|| urls.clone())
        })) {
        Some(urls) => urls,
        None => {
            let hits = probe_mirrors().await;
            // 只有「连一个 HTTP 响应都没拿到」才算真·网络不可达；
            // 全线 404 之类属于"仓库尚无发布版本"，仍走插件检查以给出准确文案。
            if hits.is_empty() {
                let err = UpdateError::new(
                    UpdateErrKind::Network,
                    "无法连接更新服务器（直连与全部加速镜像均不可达，请检查本机网络）",
                );
                finish_failed(app, shared, &err);
                return Err(err);
            }
            let urls: Vec<Url> = rank_probe_hits(&hits);
            if let Ok(mut g) = shared.lock() {
                g.probe_cache = Some((urls.clone(), Instant::now()));
            }
            urls
        }
    };

    // 2) 插件拉清单 + semver 比较（验签发生在下载阶段）。
    let updater = match build_updater(app, &endpoints, CHECK_TIMEOUT) {
        Ok(u) => u,
        Err(e) => {
            finish_failed(app, shared, &e);
            return Err(e);
        }
    };
    let update = match updater.check().await {
        Ok(found) => found,
        Err(e) => {
            let mut classified = classify_error(&e.to_string());
            if classified.kind == UpdateErrKind::NotFound {
                // 直连+镜像都响应了但清单缺失 = 还没有发布过任何版本。
                classified.message =
                    "还没有可用的已发布版本（首次 Release 发布后即可正常检查更新）".into();
            }
            if let Ok(mut g) = shared.lock() {
                g.probe_cache = None; // 该镜像可能已坏，下次重新探测
            }
            finish_failed(app, shared, &classified);
            return Err(classified);
        }
    };

    let phase = match update {
        None => P::UpToDate { current: app.package_info().version.to_string() },
        Some(u) => {
            if let Ok(mut g) = shared.lock() {
                g.pending_endpoints = Some(endpoints);
            }
            P::Available {
                current: u.current_version.clone(),
                version: u.version.clone(),
                notes: u.body.clone(),
            }
        }
    };
    set_phase_shared(app, shared, phase.clone());
    Ok(phase)
}

/// 下载并安装已发现的更新，成功后由安装器接管重启（Windows NSIS）。
///
/// 立即返回；进度经 updater://progress / updater://phase 事件推送；
/// 传输类失败自动轮换次优镜像重试（最多 MAX_RETRIES 次）。
/// 并发安全：busy 标志为锁内原子 check-and-set，重复触发幂等返回。
#[tauri::command]
pub async fn update_download_cmd(
    app: AppHandle,
    state: tauri::State<'_, UpdateState>,
) -> Result<(), UpdateError> {
    if !state.try_begin_busy() {
        return Ok(()); // 已有后台任务在跑：幂等返回，前端经事件接续即可。
    }
    let shared = state.shared();

    // 未处于「发现新版本」状态则先补一次检查（在后台流程内，保持 busy 防止并发）。
    let need_check = !matches!(
        current_phase_shared(&shared),
        Some(UpdatePhase::Available { .. })
    );
    if need_check {
        let app2 = app.clone();
        let shared2 = shared.clone();
        tauri::async_runtime::spawn(async move {
            match check_flow(&app2, &shared2).await {
                Ok(UpdatePhase::Available { .. }) => {
                    let endpoints = shared2
                        .lock()
                        .ok()
                        .and_then(|g| g.pending_endpoints.clone())
                        .unwrap_or_default();
                    set_phase_shared(
                        &app2,
                        &shared2,
                        UpdatePhase::Downloading { downloaded: 0, total: None, retrying: false },
                    );
                    download_flow(app2, shared2, endpoints).await;
                }
                Ok(_) => {
                    // 已是最新 / 无可用更新：结束流程
                    if let Ok(mut g) = shared2.lock() {
                        g.busy = false;
                    }
                    set_phase_shared(&app2, &shared2, UpdatePhase::Idle);
                }
                Err(_) => {
                    // check_flow 内部已 finish_failed 并清 busy
                }
            }
        });
        return Ok(());
    }

    let endpoints = shared
        .lock()
        .ok()
        .and_then(|g| g.pending_endpoints.clone())
        .ok_or_else(|| UpdateError::new(UpdateErrKind::Unknown, "没有待安装的更新，请先检查更新"))?;
    set_phase_shared(&app, &shared, UpdatePhase::Downloading { downloaded: 0, total: None, retrying: false });
    let app2 = app.clone();
    tauri::async_runtime::spawn(async move {
        download_flow(app2, shared, endpoints).await;
    });
    Ok(())
}

/// 当前更新状态快照（前端打开设置页时重接进行中的流程）。
#[tauri::command]
pub fn update_state_cmd(state: tauri::State<'_, UpdateState>) -> Option<UpdatePhase> {
    state.current_phase()
}

// ───────────────────────── 下载主流程 ─────────────────────────

/// 下载→验签→安装→重启 的完整后台流程，含镜像轮换重试。
async fn download_flow(app: AppHandle, state: Arc<Mutex<StateInner>>, endpoints: Vec<Url>) {
    use UpdatePhase as P;

    let result = run_attempts(&app, &state, &endpoints).await;

    // 收尾：只有失败路径会走到这里（成功路径已 restart 分离进程）。
    if let Some(err) = result {
        if let Ok(mut g) = state.lock() {
            g.busy = false;
        }
        set_phase_shared(&app, &state, P::Failed {
            code: err.kind.clone(),
            message: err.message.clone(),
            manual_url: err.manual_url.clone(),
        });
    }
}

/// 尝试循环：Ok(()) 表示已进入安装/重启分支；Some(err)= 最终失败。
async fn run_attempts(
    app: &AppHandle,
    state: &Arc<Mutex<StateInner>>,
    endpoints: &[Url],
) -> Option<UpdateError> {
    use UpdatePhase as P;

    for attempt in 0..=MAX_RETRIES {
        // 主选轮换：第 k 次尝试以第 k%N 个镜像为主选，其余保持相对顺序兜底。
        let rotated = rotate_by(endpoints, attempt as usize);
        let mut builder = match app
            .updater_builder()
            .endpoints(rotated)
            .map_err(|e| classify_error(&e.to_string()))
        {
            Ok(b) => b,
            Err(e) => return Some(e),
        };
        // Windows NSIS 安装流程插件会 `std::process::exit(0)`（下载超时后不返回），
        // 项目的 Installing/Restarting/restart 永远执行不到——改为在插件的
        // on_before_exit 钩子（exit 前回调）补发「安装中/即将重启」事件并清 busy，
        // 前端才能收到最终状态。
        {
            let exit_app = app.clone();
            let exit_state = state.clone();
            builder = builder.on_before_exit(move || {
                set_phase_shared(&exit_app, &exit_state, P::Installing);
                set_phase_shared(&exit_app, &exit_state, P::Restarting);
                if let Ok(mut g) = exit_state.lock() {
                    g.busy = false;
                }
            });
        }
        let updater = match builder
            .timeout(DOWNLOAD_TIMEOUT)
            .build()
            .map_err(|e| classify_error(&e.to_string()))
        {
            Ok(u) => u,
            Err(e) => return Some(e),
        };
        let update = match updater.check().await {
            Ok(Some(u)) => u,
            Ok(None) => {
                set_phase_shared(app, state, P::UpToDate {
                    current: app.package_info().version.to_string(),
                });
                return None;
            }
            Err(e) => {
                let classified = classify_error(&e.to_string());
                // 验签失败/资源缺失/不可重试类 → 立即终结；传输类 → 换镜像重试
                if plan_retry(classified.kind, attempt, MAX_RETRIES).is_none() {
                    return Some(classified);
                }
                continue;
            }
        };

        set_phase_shared(app, state, P::Downloading {
            downloaded: 0,
            total: None,
            retrying: attempt > 0,
        });

        // 进度聚合：插件回调给的是「每个分块长度」，这里累加并节流上报。
        let app_for_progress = app.clone();
        let downloaded = Arc::new(Mutex::new(0u64));
        let last_emit = Arc::new(Mutex::new(Instant::now() - Duration::from_millis(200)));
        let result = {
            let downloaded = downloaded.clone();
            let last_emit = last_emit.clone();
            update
                .download_and_install(
                    move |chunk_len, total| {
                        let mut acc = downloaded.lock().unwrap();
                        *acc += chunk_len as u64;
                        let downloaded_now = *acc;
                        drop(acc);
                        let mut last = last_emit.lock().unwrap();
                        if last.elapsed() >= Duration::from_millis(100) {
                            *last = Instant::now();
                            drop(last);
                            let _ = app_for_progress.emit(
                                "updater://progress",
                                ProgressPayload { downloaded: downloaded_now, total },
                            );
                        }
                    },
                    || {},
                )
                .await
        };

        match result {
            Ok(()) => {
                // Windows 上插件已接管安装并 exit(0)（on_before_exit 已发事件）。
                // 此分支仅为防御性收尾（非 Windows 或插件未来行为变化）。
                set_phase_shared(app, state, P::Installing);
                return None;
            }
            Err(e) => {
                let classified = classify_error(&e.to_string());
                match plan_retry(classified.kind, attempt, MAX_RETRIES) {
                    None => return Some(classified),
                    Some(_) => {
                        // 指数退避 1s / 2s 后换主选镜像重试。
                        tokio::time::sleep(Duration::from_secs(1 << attempt)).await;
                    }
                }
            }
        }
    }
    None // 循环内必已 return；此处仅为类型闭合。
}

/// 单次尝试失败后的重试决策（纯函数，单测覆盖）：
/// Signature/NotFound/Install 重试无意义 → 终结；Network/Unknown 在上限内续重试。
fn plan_retry(kind: UpdateErrKind, attempt: u32, max: u32) -> Option<u32> {
    if matches!(kind, UpdateErrKind::Signature | UpdateErrKind::NotFound | UpdateErrKind::Install) {
        return None;
    }
    if attempt >= max {
        return None;
    }
    Some(attempt + 1)
}

#[derive(Clone, Serialize)]
struct ProgressPayload {
    downloaded: u64,
    total: Option<u64>,
}

/// 将端点表旋转 k 位（主选轮换，其余保持相对顺序作为插件级兜底链）。纯函数。
fn rotate_by(urls: &[Url], k: usize) -> Vec<Url> {
    if urls.is_empty() {
        return Vec::new();
    }
    let n = k % urls.len();
    urls.iter().skip(n).cloned().chain(urls.iter().take(n).cloned()).collect()
}

fn set_phase_shared(app: &AppHandle, state: &Arc<Mutex<StateInner>>, phase: UpdatePhase) {
    if let Ok(mut g) = state.lock() {
        g.phase = Some(phase.clone());
    }
    let _ = app.emit("updater://phase", &phase);
}

fn finish_failed(app: &AppHandle, state: &Arc<Mutex<StateInner>>, err: &UpdateError) {
    if let Ok(mut g) = state.lock() {
        g.busy = false;
    }
    set_phase_shared(app, state, UpdatePhase::Failed {
        code: err.kind.clone(),
        message: err.message.clone(),
        manual_url: err.manual_url.clone(),
    });
}

// ───────────────────────── 测试 ─────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn manifest_url_direct_and_mirrors() {
        let direct = manifest_url(0).unwrap();
        assert_eq!(
            direct.as_str(),
            "https://github.com/Ljinzhou/oii_sticker/releases/latest/download/latest.json"
        );
        let m1 = manifest_url(1).unwrap();
        assert!(m1.as_str().starts_with("https://gh-proxy.org/https://github.com/"));
        assert!(m1.as_str().ends_with("/latest.json"));
        assert!(manifest_url(MIRROR_PREFIXES.len()).is_none());
    }

    #[test]
    fn rotate_keeps_order_and_wraps() {
        let urls: Vec<Url> = ["https://a/", "https://b/", "https://c/"]
            .iter().map(|s| Url::parse(s).unwrap()).collect();
        assert_eq!(rotate_by(&urls, 0)[0].as_str(), "https://a/");
        assert_eq!(rotate_by(&urls, 1)[0].as_str(), "https://b/");
        assert_eq!(rotate_by(&urls, 1)[2].as_str(), "https://a/");
        assert_eq!(rotate_by(&urls, 3)[0].as_str(), "https://a/"); // 取模回绕
        assert!(rotate_by(&[], 0).is_empty());
    }

    #[test]
    fn classify_maps_kinds() {
        assert_eq!(classify_error("signature verification failed").kind, UpdateErrKind::Signature);
        assert_eq!(classify_error("HTTP status 404 not found").kind, UpdateErrKind::NotFound);
        assert!(matches!(classify_error("msiexec exited with 1603").kind, UpdateErrKind::Install));
        assert_eq!(classify_error("connection reset by peer").kind, UpdateErrKind::Network);
        assert_eq!(classify_error("request timeout").kind, UpdateErrKind::Network);
        assert_eq!(classify_error("weird unknown").kind, UpdateErrKind::Unknown);
    }

    #[test]
    fn classify_error_carries_manual_url_and_chinese_message() {
        let e = classify_error("connection refused");
        assert_eq!(e.manual_url, MANUAL_RELEASES_URL);
        assert!(e.message.starts_with("网络异常"));
    }

    #[test]
    fn rank_puts_direct_first_then_ok_by_latency_404_last_dead_dropped() {
        let u = |i| manifest_url(i).unwrap();
        // 0=直连(慢但ok) 1=gh-proxy(ok快) 2=v4(404有响应) 3=v6(传输失败)
        let hits = vec![
            ProbeHit { endpoint: u(0), latency_ms: 900, responded: true, ok: true },
            ProbeHit { endpoint: u(1), latency_ms: 120, responded: true, ok: true },
            ProbeHit { endpoint: u(2), latency_ms: 80, responded: true, ok: false },
            ProbeHit { endpoint: u(3), latency_ms: 5, responded: false, ok: false },
        ];
        let ranked = rank_probe_hits(&hits);
        assert_eq!(ranked[0].as_str(), manifest_url(0).unwrap().as_str());
        assert_eq!(ranked[1].as_str(), manifest_url(1).unwrap().as_str());
        assert_eq!(ranked[2].as_str(), manifest_url(2).unwrap().as_str());
        assert_eq!(ranked.len(), 3); // 传输失败的 v6 被剔除
    }

    #[test]
    fn rank_all_404_still_returns_responded_endpoints() {
        let hits: Vec<ProbeHit> = [1usize, 0]
            .iter()
            .map(|&i| ProbeHit { endpoint: u_helper(i), latency_ms: 100, responded: true, ok: false })
            .collect();
        let ranked = rank_probe_hits(&hits);
        assert_eq!(ranked.len(), 2);
        // 直连即便全 404 也排第一
        assert_eq!(ranked[0].as_str(), manifest_url(0).unwrap().as_str());
    }

    fn u_helper(i: usize) -> Url {
        manifest_url(i).unwrap()
    }

    #[test]
    fn plan_retry_terminates_unretryable_and_exhausts_cap() {
        // 无意义重试类一律终结
        assert_eq!(plan_retry(UpdateErrKind::Signature, 0, 2), None);
        assert_eq!(plan_retry(UpdateErrKind::NotFound, 0, 2), None);
        assert_eq!(plan_retry(UpdateErrKind::Install, 0, 2), None);
        // 传输类在上限内续重试
        assert_eq!(plan_retry(UpdateErrKind::Network, 0, 2), Some(1));
        assert_eq!(plan_retry(UpdateErrKind::Unknown, 1, 2), Some(2));
        // 到达上限终结
        assert_eq!(plan_retry(UpdateErrKind::Network, 2, 2), None);
    }

    #[test]
    fn extend_prefixes_keeps_direct_first_and_cleans_extra() {
        let base = ["", "https://m1/"];
        // 空 → 仅内置
        assert_eq!(extend_prefixes(&base, ""), vec!["", "https://m1/"]);
        // 清洗：去空白/尾部斜杠、空项过滤；追加在直连之后
        let got = extend_prefixes(&base, "  https://mirror.internal/ , https://m2.test,,");
        assert_eq!(
            got,
            vec!["", "https://m1/", "https://mirror.internal/", "https://m2.test/"]
        );
    }

    #[test]
    fn classify_maps_disk_full_and_certificate() {
        let e = classify_error("failed to write file: no space left on device");
        assert_eq!(e.kind, UpdateErrKind::Install);
        assert!(e.message.contains("磁盘空间不足"));
        let e = classify_error("certificate verify failed");
        assert_eq!(e.kind, UpdateErrKind::Network);
    }
}
