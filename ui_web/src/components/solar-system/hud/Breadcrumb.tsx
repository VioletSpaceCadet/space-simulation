interface BreadcrumbProps {
  focusName: string | null;
  onReset: () => void;
}

export function Breadcrumb({ focusName, onReset }: BreadcrumbProps) {
  if (!focusName) { return null; }

  return (
    <div className="bg-void/88 border border-edge rounded px-2.5 py-1.5 backdrop-blur-sm text-[10px] mt-1.5">
      <div className="flex items-center gap-1 text-dim">
        <button
          type="button"
          className="cursor-pointer hover:text-accent"
          onClick={onReset}
        >
          System
        </button>
        <span className="text-faint">&rsaquo;</span>
        <span className="text-accent">{focusName}</span>
      </div>
    </div>
  );
}
