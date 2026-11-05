/** The 16 px error mark that precedes an error message; the message carries the meaning. */
export function ErrorIcon() {
  return (
    <svg width="16" height="16" viewBox="0 0 16 16" aria-hidden="true">
      <circle cx="8" cy="8" r="6.5" fill="none" stroke="currentColor" />
      <path d="M8 4.5v4M8 10.5v.01" stroke="currentColor" strokeWidth="1.5" />
    </svg>
  );
}
