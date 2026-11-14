// Line icons of the 3D view (Icons page of the design: Tool / Select, Move, Block; UI / Zoom to
// fit, Zoom to selection, View presets). Decorative: the buttons carry the names.

const Icon = ({ size, children }: { size: number; children: React.ReactNode }) => (
  <svg
    width={size}
    height={size}
    viewBox="0 0 20 20"
    fill="none"
    stroke="currentColor"
    strokeWidth="1.5"
    strokeLinejoin="round"
    aria-hidden="true"
  >
    {children}
  </svg>
);

export const SelectIcon = () => (
  <Icon size={20}>
    <path d="M5 3l10 7-4.5 1L8 15.5z" />
  </Icon>
);

export const MoveIcon = () => (
  <Icon size={20}>
    <path d="M10 2v16M2 10h16M10 2l-2.5 2.5M10 2l2.5 2.5M10 18l-2.5-2.5M10 18l2.5-2.5M2 10l2.5-2.5M2 10l2.5 2.5M18 10l-2.5-2.5M18 10l-2.5 2.5" />
  </Icon>
);

export const BlockIcon = () => (
  <Icon size={20}>
    <path d="M10 2.5l6.5 3.5v8L10 17.5 3.5 14V6zM3.5 6L10 9.5 16.5 6M10 9.5v8" />
  </Icon>
);

export const ZoomFitIcon = () => (
  <Icon size={16}>
    <path d="M3 7V3h4M13 3h4v4M17 13v4h-4M7 17H3v-4" />
  </Icon>
);

export const ZoomSelectionIcon = () => (
  <Icon size={16}>
    <path d="M3 7V3h4M13 3h4v4M17 13v4h-4M7 17H3v-4" />
    <rect x="7" y="7" width="6" height="6" />
  </Icon>
);

export const PresetsIcon = () => (
  <Icon size={16}>
    <path d="M9 2.5l6 3.2v7L9 16l-6-3.3v-7zM3 5.7L9 9l6-3.3M9 9v7M15.5 13.5l2 2 2-2" />
  </Icon>
);
