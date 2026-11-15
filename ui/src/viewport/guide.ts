import { inDesktopWindow, openInSystemBrowser } from "../api";

// The user guide pages the 3D view points to (docs/user/). The guide is not bundled with the
// application yet, so links open the published sources.
const GUIDE = "https://github.com/GuillaumeOj/BIMCraft/blob/main/docs/user/";

export const GUIDE_TROUBLESHOOTING = `${GUIDE}navigate-the-3d-view.md#troubleshooting`;
export const GUIDE_PLACE_BLOCK = `${GUIDE}place-a-block.md`;

/**
 * Opens the guide page `url` for a click on its link. In the desktop window a link would open
 * inside the application, so the click is handed to the system browser (the opener plugin allows
 * only these guide pages); `onFailed` hears of a refusal. In a browser the link works as it is,
 * and this does nothing.
 */
export function openGuide(
  event: { preventDefault: () => void },
  url: string,
  onFailed: (url: string) => void,
): void {
  if (!inDesktopWindow()) return;
  event.preventDefault();
  openInSystemBrowser(url).catch(() => onFailed(url));
}
