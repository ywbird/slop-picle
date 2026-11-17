// Drag a floating card by its header: calls `move` with the offset from where the drag started.
export function dragOffset(e: PointerEvent, start: { x: number; y: number }, move: (x: number, y: number) => void) {
  if ((e.target as HTMLElement).closest("button, input, a, select")) return;
  const sx = e.clientX - start.x;
  const sy = e.clientY - start.y;
  const onMove = (m: PointerEvent) => move(m.clientX - sx, m.clientY - sy);
  const onUp = () => {
    window.removeEventListener("pointermove", onMove);
    window.removeEventListener("pointerup", onUp);
  };
  window.addEventListener("pointermove", onMove);
  window.addEventListener("pointerup", onUp);
}
