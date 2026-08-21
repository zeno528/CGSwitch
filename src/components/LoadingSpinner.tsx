export function LoadingSpinner({ size = "sm" }: { size?: "sm" | "md" }) {
  return (
    <svg className={`animate-spin ${size === "md" ? "h-4 w-4" : "h-3 w-3"}`} viewBox="0 0 24 24" fill="none" aria-hidden="true">
      <circle cx="12" cy="12" r="9" stroke="currentColor" strokeOpacity="0.25" strokeWidth="2.5" />
      <path d="M21 12a9 9 0 0 0-9-9" stroke="currentColor" strokeWidth="2.5" strokeLinecap="round" />
    </svg>
  );
}
