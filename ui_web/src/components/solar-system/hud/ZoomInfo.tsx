import type { Camera } from '../canvas/types';

interface ZoomInfoProps {
  camera: Camera;
}

function scaleLabel(zoom: number): string {
  // How many AU does a 60px bar represent?
  const auPerBar = 60 / (zoom * 200); // 200 world units per AU
  if (auPerBar > 100) {
    return `${Math.round(auPerBar).toLocaleString()} AU`;
  }
  if (auPerBar < 0.1) {
    return `${(auPerBar * 1000).toFixed(0)} mAU`;
  }
  return `${auPerBar.toFixed(1)} AU`;
}

function lodLabel(zoom: number): string {
  if (zoom < 0.15) { return 'SYSTEM'; }
  if (zoom < 0.8) { return 'REGION'; }
  return 'LOCAL';
}

export function ZoomInfo({ camera }: ZoomInfoProps) {
  return (
    <div className="absolute top-4 left-4 z-10 pointer-events-none">
      <div className="bg-void/88 border border-edge rounded px-2.5 py-1.5 backdrop-blur-sm text-[10px] text-muted tracking-wider">
        ZOOM <span className="text-accent font-medium">{camera.zoom.toFixed(1)}x</span>
        <span className="text-faint ml-1">&middot; {lodLabel(camera.zoom)}</span>
      </div>
      <div className="bg-void/88 border border-edge rounded px-2.5 py-1.5 backdrop-blur-sm text-[10px] text-muted mt-1.5 flex items-center gap-2">
        <div className="relative w-[60px] h-px bg-muted">
          <div className="absolute left-0 -top-0.5 w-px h-1.5 bg-muted" />
          <div className="absolute right-0 -top-0.5 w-px h-1.5 bg-muted" />
        </div>
        <span>{scaleLabel(camera.zoom)}</span>
      </div>
    </div>
  );
}
