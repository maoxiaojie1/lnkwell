<script lang="ts">
  interface Props {
    src: string;
    onClose: () => void;
  }
  let { src, onClose }: Props = $props();

  let scale = $state(1);
  let tx = $state(0);
  let ty = $state(0);

  /** 加载状态：解码失败时给出提示，否则纯黑一片分不清「不支持」还是「加载失败」 */
  let status = $state<'loading' | 'ready' | 'failed'>('loading');
  $effect(() => {
    src;
    status = 'loading';
  });

  const pointers = new Map<number, { x: number; y: number }>();
  let pinchDist = 0;
  let moved = false;

  function clampScale(v: number): number {
    return Math.min(8, Math.max(0.2, v));
  }

  function onWheel(e: WheelEvent) {
    e.preventDefault();
    scale = clampScale(scale * (e.deltaY < 0 ? 1.15 : 1 / 1.15));
    if (scale === 1) {
      tx = 0;
      ty = 0;
    }
  }

  function onPointerDown(e: PointerEvent) {
    (e.currentTarget as HTMLElement).setPointerCapture(e.pointerId);
    pointers.set(e.pointerId, { x: e.clientX, y: e.clientY });
    moved = false;
    if (pointers.size === 2) {
      const [a, b] = [...pointers.values()];
      pinchDist = Math.hypot(a.x - b.x, a.y - b.y);
    }
  }

  function onPointerMove(e: PointerEvent) {
    const prev = pointers.get(e.pointerId);
    if (!prev) return;
    if (pointers.size === 1) {
      // 单指拖拽平移
      tx += e.clientX - prev.x;
      ty += e.clientY - prev.y;
      if (Math.abs(e.clientX - prev.x) + Math.abs(e.clientY - prev.y) > 2) moved = true;
      pointers.set(e.pointerId, { x: e.clientX, y: e.clientY });
    } else if (pointers.size === 2) {
      // 双指捏合缩放
      pointers.set(e.pointerId, { x: e.clientX, y: e.clientY });
      const [a, b] = [...pointers.values()];
      const d = Math.hypot(a.x - b.x, a.y - b.y);
      if (pinchDist > 0) scale = clampScale(scale * (d / pinchDist));
      pinchDist = d;
      moved = true;
    }
  }

  function onPointerUp(e: PointerEvent) {
    pointers.delete(e.pointerId);
    if (pointers.size < 2) pinchDist = 0;
    if (pointers.size === 0 && scale <= 1) {
      scale = 1;
      tx = 0;
      ty = 0;
    }
  }

  function onDblClick() {
    if (scale > 1) {
      scale = 1;
      tx = 0;
      ty = 0;
    } else {
      scale = 2.5;
    }
  }

  function onBackdropClick(e: MouseEvent) {
    // 只在点击遮罩本身且未发生拖拽时关闭，避免拖完图片误关
    if (e.target === e.currentTarget && !moved) onClose();
  }

  // 键盘可达：Esc 关闭，方向键平移，+/- 缩放
  function onKeydown(e: KeyboardEvent) {
    if (e.key === 'Escape') onClose();
    else if (e.key === 'ArrowLeft') tx -= 40;
    else if (e.key === 'ArrowRight') tx += 40;
    else if (e.key === 'ArrowUp') ty -= 40;
    else if (e.key === 'ArrowDown') ty += 40;
    else if (e.key === '+' || e.key === '=') scale = clampScale(scale * 1.15);
    else if (e.key === '-') scale = clampScale(scale / 1.15);
    else return;
    e.preventDefault();
  }

  // 打开时自动聚焦，使键盘操作立即生效
  function autofocus(el: HTMLElement) {
    el.focus();
  }
</script>

<div
  class="image-viewer"
  role="dialog"
  aria-label="图片查看器"
  tabindex="-1"
  use:autofocus
  onclick={onBackdropClick}
  onkeydown={onKeydown}
  onwheel={onWheel}
  onpointerdown={onPointerDown}
  onpointermove={onPointerMove}
  onpointerup={onPointerUp}
  onpointercancel={onPointerUp}
>
  <img
    {src}
    alt=""
    draggable="false"
    style:transform="translate({tx}px, {ty}px) scale({scale})"
    ondblclick={onDblClick}
    onload={() => (status = 'ready')}
    onerror={() => (status = 'failed')}
  />
  {#if status !== 'ready'}
    <div class="viewer-hint">{status === 'failed' ? '无法显示该图片' : '加载中…'}</div>
  {/if}
</div>
