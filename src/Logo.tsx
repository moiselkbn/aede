import logo from "./assets/aede-logo.svg";
import { APP_NAME } from "./appName";

/** The Aède wordmark. The image only gives the shape: it takes the label color, so it is
 *  black in light mode and white in dark mode. */
function Logo({ height }: { height: number }) {
  return (
    <div
      className="logo"
      role="img"
      aria-label={APP_NAME}
      style={{
        height,
        // Width of the drawing (viewBox 163.73 x 56.15) at this height.
        width: Math.round((height * 163.73) / 56.15),
        // Quoted: Vite inlines the small SVG as a data URL with spaces in it.
        WebkitMaskImage: `url("${logo}")`,
        maskImage: `url("${logo}")`,
      }}
    />
  );
}

export default Logo;
