// Entry point of the indicator window (indicator.html), like main.tsx for the main window.

import React from "react";
import ReactDOM from "react-dom/client";
import "@fontsource-variable/geist/wght.css";
import Indicator from "./Indicator";

// The font file is only read when a text first uses it: ask for it now, so that the first text
// the pill shows is already in Geist and the pill has its final size.
document.fonts.load('400 12px "Geist Variable"', "éèàçœ→");

ReactDOM.createRoot(document.getElementById("root") as HTMLElement).render(
  <React.StrictMode>
    <Indicator />
  </React.StrictMode>,
);
