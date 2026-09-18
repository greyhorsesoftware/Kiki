import QtQuick
import QtQuick.Shapes
import ".." as Kiki

// kiki's mark: the wireframe cat, drawn rather than loaded.
//
// The artwork is a one-colour line drawing stroked in near-black (`app-images/`), which vanishes
// on every dark Omarchy theme. Tinting a raster of it costs sharpness — a stencil is only as
// crisp as the texture behind it — so the same geometry is drawn here as vector paths in the
// theme's colour. Sharp at any size, and it follows every theme for free.
Item {
    id: mark
    implicitWidth: 112; implicitHeight: 112

    property color color: Kiki.Theme.accent
    /// Full accent shouts; a little under reads as a mark rather than a warning light.
    property real strength: 0.78
    /// The drawing is 512 across; everything scales from that.
    readonly property real unit: Math.min(width, height) / 512

    Shape {
        anchors.centerIn: parent
        width: 512 * mark.unit; height: 512 * mark.unit
        preferredRendererType: Shape.CurveRenderer     // analytic curves rather than a texture
        opacity: mark.strength
        transform: Scale { xScale: mark.unit; yScale: mark.unit }

        component Stroke: ShapePath {
            strokeColor: mark.color
            strokeWidth: 18
            fillColor: "transparent"
            capStyle: ShapePath.RoundCap
            joinStyle: ShapePath.RoundJoin
        }

        // Head with ears, inner ears, muzzle, mouth, whiskers.
        Stroke { PathSvg { path: "M 110 190 L 96 74 Q 96 58 110 66 L 190 126 Q 256 104 322 126 L 402 66 Q 416 58 416 74 L 402 190 Q 448 262 428 344 Q 398 430 256 444 Q 114 430 84 344 Q 64 262 110 190 Z" } }
        Stroke { PathSvg { path: "M 128 104 L 138 176 Q 166 148 200 138" } }
        Stroke { PathSvg { path: "M 384 104 L 374 176 Q 346 148 312 138" } }
        Stroke { PathSvg { path: "M 152 296 Q 200 268 256 274 Q 312 268 360 296" } }
        Stroke { PathSvg { path: "M 238 306 L 274 306 L 256 328 Z" } }
        Stroke { PathSvg { path: "M 256 328 L 256 346 Q 236 364 220 350" } }
        Stroke { PathSvg { path: "M 256 346 Q 276 364 292 350" } }
        Stroke { PathSvg { path: "M 172 326 L 100 314" } }
        Stroke { PathSvg { path: "M 174 348 L 102 354" } }
        Stroke { PathSvg { path: "M 340 326 L 412 314" } }
        Stroke { PathSvg { path: "M 338 348 L 410 354" } }

        // Eyes: an outline and a filled pupil each.
        Stroke { PathSvg { path: "M 164 228 A 30 22 0 1 1 224 228 A 30 22 0 1 1 164 228 Z" } }
        Stroke { PathSvg { path: "M 288 228 A 30 22 0 1 1 348 228 A 30 22 0 1 1 288 228 Z" } }
        ShapePath {
            fillColor: mark.color; strokeColor: "transparent"
            PathSvg { path: "M 183 228 A 11 11 0 1 1 205 228 A 11 11 0 1 1 183 228 Z" }
        }
        ShapePath {
            fillColor: mark.color; strokeColor: "transparent"
            PathSvg { path: "M 307 228 A 11 11 0 1 1 329 228 A 11 11 0 1 1 307 228 Z" }
        }
    }
}
