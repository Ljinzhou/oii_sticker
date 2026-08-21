<script setup lang="ts">
import { nextTick, onBeforeUnmount, onMounted, ref } from "vue";

const props = defineProps<{ anchor: DOMRect }>();
const emit = defineEmits<{ close: [source?: Event] }>();
const panel = ref<HTMLElement | null>(null);
const pos = ref({ top: -9999, left: 0 });

function place() {
  const el = panel.value;
  if (!el) return;
  const width = el.offsetWidth || 280;
  const height = el.offsetHeight || 360;
  const left = Math.min(Math.max(props.anchor.left, 8), Math.max(8, window.innerWidth - width - 8));
  const below = props.anchor.bottom + 6;
  const top = below + height <= window.innerHeight - 8 ? below : Math.max(8, props.anchor.top - height - 6);
  pos.value = { top, left };
}

function onPointerDown(event: PointerEvent) {
  if (panel.value && event.target instanceof Node && !panel.value.contains(event.target)) emit("close", event);
}
function onKeydown(event: KeyboardEvent) { if (event.key === "Escape") emit("close"); }
const onResize = () => emit("close");

onMounted(async () => {
  await nextTick();
  place();
  document.addEventListener("pointerdown", onPointerDown, true);
  document.addEventListener("keydown", onKeydown);
  window.addEventListener("resize", onResize);
});
onBeforeUnmount(() => {
  document.removeEventListener("pointerdown", onPointerDown, true);
  document.removeEventListener("keydown", onKeydown);
  window.removeEventListener("resize", onResize);
});
</script>

<template>
  <Teleport to="body">
    <div ref="panel" class="picker-float" :style="{ top: pos.top + 'px', left: pos.left + 'px' }" @click.stop>
      <slot />
    </div>
  </Teleport>
</template>

<style scoped>
.picker-float { position: fixed; z-index: 1000; width: max-content; }
.picker-float :deep(.date-picker), .picker-float :deep(.repeat-picker) { position: static; }
</style>
