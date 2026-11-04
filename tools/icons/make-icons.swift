// Draws the app icon and the menu bar icon from tools/icons/logo.svg.
// Run it from the repository root: `swift tools/icons/make-icons.swift`,
// then `npm run tauri icon tools/icons/app-icon.png` for the app icon sizes.
//
// Writes:
// - tools/icons/app-icon.png (1024 px), from which `tauri icon` makes icon.icns and the other sizes
// - src-tauri/icons/tray.png, the menu bar icon (a template image: only the transparency
//   counts, macOS paints it in the colour of the menu bar)

import AppKit

let logo = NSImage(contentsOfFile: "tools/icons/logo.svg")!
// The logo's own proportions (viewBox of the SVG).
let logoRatio = 89.76 / 56.16

/// Draws into a transparent bitmap of the given size and saves it as a PNG.
func savePNG(width: Int, height: Int, to path: String, draw: (CGContext) -> Void) {
    let rep = NSBitmapImageRep(
        bitmapDataPlanes: nil, pixelsWide: width, pixelsHigh: height,
        bitsPerSample: 8, samplesPerPixel: 4, hasAlpha: true, isPlanar: false,
        colorSpaceName: .deviceRGB, bytesPerRow: 0, bitsPerPixel: 0)!
    NSGraphicsContext.saveGraphicsState()
    let context = NSGraphicsContext(bitmapImageRep: rep)!
    NSGraphicsContext.current = context
    draw(context.cgContext)
    NSGraphicsContext.restoreGraphicsState()
    try! rep.representation(using: .png, properties: [:])!.write(to: URL(fileURLWithPath: path))
}

// App icon: the logo on a light rounded square, with the margins of the macOS icon grid
// (an 824 px square in a 1024 px canvas, so the icon is the same size as the others in the Dock).
savePNG(width: 1024, height: 1024, to: "tools/icons/app-icon.png") { cg in
    let square = CGRect(x: 100, y: 100, width: 824, height: 824)
    let shape = CGPath(roundedRect: square, cornerWidth: 185, cornerHeight: 185, transform: nil)

    cg.saveGState()
    cg.setShadow(offset: CGSize(width: 0, height: -10), blur: 20,
                 color: NSColor.black.withAlphaComponent(0.3).cgColor)
    cg.addPath(shape)
    cg.setFillColor(NSColor.white.cgColor)
    cg.fillPath()
    cg.restoreGState()

    cg.saveGState()
    cg.addPath(shape)
    cg.clip()
    let gradient = CGGradient(
        colorsSpace: CGColorSpaceCreateDeviceRGB(),
        colors: [NSColor(white: 1, alpha: 1).cgColor, NSColor(white: 0.9, alpha: 1).cgColor] as CFArray,
        locations: [0, 1])!
    cg.drawLinearGradient(gradient, start: CGPoint(x: 0, y: 924), end: CGPoint(x: 0, y: 100), options: [])
    cg.restoreGState()

    let logoWidth = 560.0
    let logoHeight = logoWidth / logoRatio
    logo.draw(in: NSRect(x: 512 - logoWidth / 2, y: 512 - logoHeight / 2, width: logoWidth, height: logoHeight))
}

// Menu bar icon. macOS always shows it 18 points high, so 36 px is sharp on a Retina
// screen; the logo is drawn smaller than that, with empty space above and below.
let trayHeight = 36.0
let logoHeight = 24.0
let logoWidth = logoHeight * logoRatio
let trayWidth = (logoWidth + 2).rounded(.up)

savePNG(width: Int(trayWidth), height: Int(trayHeight), to: "src-tauri/icons/tray.png") { _ in
    logo.draw(in: NSRect(x: 1, y: (trayHeight - logoHeight) / 2, width: logoWidth, height: logoHeight))
}
