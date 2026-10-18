import QtQuick

// Tsuba's own line icons for lighting effects (drawn with Canvas; no external artwork).
Canvas {
    id: icon

    property string type: "static"
    property bool active: false
    property color stroke: active ? Theme.accent : Theme.text
    property real size: 24

    implicitWidth: size
    implicitHeight: size
    width: size
    height: size
    onTypeChanged: requestPaint()
    onStrokeChanged: requestPaint()
    onSizeChanged: requestPaint()

    onPaint: {
        const ctx = getContext("2d");
        ctx.reset();
        const s = width / 24;
        ctx.scale(s, s);
        ctx.strokeStyle = stroke;
        ctx.fillStyle = stroke;
        ctx.lineWidth = 1.5;
        ctx.lineCap = "round";
        ctx.lineJoin = "round";
        const line = (pts, close) => {
            ctx.beginPath();
            ctx.moveTo(pts[0], pts[1]);
            for (let i = 2; i < pts.length; i += 2) {
                ctx.lineTo(pts[i], pts[i + 1]);
            }
            if (close) {
                ctx.closePath();
            }
            ctx.stroke();
        };
        const circle = (x, y, r, fill) => {
            ctx.beginPath();
            ctx.arc(x, y, r, 0, Math.PI * 2);
            fill ? ctx.fill() : ctx.stroke();
        };
        switch (type) {
        case "static":     // solid dot with a ring
            circle(12, 12, 8, false);
            circle(12, 12, 3.5, true);
            break;
        case "breathing":  // concentric rings fading outward
            circle(12, 12, 3, true);
            circle(12, 12, 6.5, false);
            ctx.globalAlpha = 0.5;
            circle(12, 12, 10, false);
            break;
        case "spectrum":   // three arcs, rainbow-ish
            for (let i = 0; i < 3; i++) {
                ctx.beginPath();
                ctx.arc(12, 18, 5 + i * 3.5, Math.PI, 0);
                ctx.stroke();
            }
            break;
        case "wave":
            ctx.beginPath();
            ctx.moveTo(2, 12);
            ctx.bezierCurveTo(5, 4, 8, 4, 11, 12);
            ctx.bezierCurveTo(14, 20, 17, 20, 22, 12);
            ctx.stroke();
            break;
        case "wheel":      // circular arrow
            ctx.beginPath();
            ctx.arc(12, 12, 8, -Math.PI * 0.2, Math.PI * 1.5);
            ctx.stroke();
            line([15.5, 3.2, 19.5, 5.2, 16.5, 8.6], false);
            break;
        case "starlight":  // four-point star + spark
            line([12, 3, 14, 10, 21, 12, 14, 14, 12, 21, 10, 14, 3, 12, 10, 10], true);
            break;
        case "fire":       // flame
            ctx.beginPath();
            ctx.moveTo(12, 21);
            ctx.bezierCurveTo(5, 21, 4, 14, 9, 9);
            ctx.bezierCurveTo(9, 12, 11, 12, 11, 12);
            ctx.bezierCurveTo(10, 8, 12, 4, 14, 2.5);
            ctx.bezierCurveTo(14, 8, 20, 11, 19, 16);
            ctx.bezierCurveTo(18.5, 19, 16, 21, 12, 21);
            ctx.stroke();
            break;
        case "audio_meter": // equaliser bars
            line([5, 20, 5, 13], false);
            line([9.5, 20, 9.5, 6], false);
            line([14.5, 20, 14.5, 10], false);
            line([19, 20, 19, 4], false);
            break;
        case "ambient":    // screen with glow
            ctx.strokeRect(3, 5, 18, 11);
            line([9, 20, 15, 20], false);
            line([12, 16, 12, 20], false);
            break;
        case "tidal":      // ripples from a point
            ctx.beginPath();
            ctx.arc(12, 12, 3, 0, Math.PI * 2);
            ctx.stroke();
            ctx.beginPath();
            ctx.arc(12, 12, 7, -0.9, 0.9);
            ctx.stroke();
            ctx.beginPath();
            ctx.arc(12, 12, 7, Math.PI - 0.9, Math.PI + 0.9);
            ctx.stroke();
            ctx.beginPath();
            ctx.arc(12, 12, 10.5, -0.7, 0.7);
            ctx.stroke();
            ctx.beginPath();
            ctx.arc(12, 12, 10.5, Math.PI - 0.7, Math.PI + 0.7);
            ctx.stroke();
            break;
        case "battery":
            ctx.strokeRect(3, 8, 16, 9);
            line([21, 11, 21, 14], false);
            ctx.fillRect(5, 10, 7, 5);
            break;
        default:
            circle(12, 12, 8, false);
        }
    }
}
