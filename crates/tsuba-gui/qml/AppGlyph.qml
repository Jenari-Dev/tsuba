import QtQuick

// Tsuba's own app-tab icons, drawn in the accent colour: the tsuba (sword guard) mark for Home,
// a keycap with a play arrow for Macros, a gamepad for Linked games, a prism for Lighting Studio,
// a gamepad sending out light rays for Game lighting.
Canvas {
    id: g
    property string kind: "tsuba"   // tsuba | macro | links | studio | devices | gamelight
    property color colour: Theme.accent
    implicitWidth: 18
    implicitHeight: 18
    onColourChanged: requestPaint()
    onKindChanged: requestPaint()
    onWidthChanged: requestPaint()

    onPaint: {
        const ctx = getContext("2d");
        ctx.reset();
        const s = Math.min(width, height) / 64;   // all glyphs are drawn on a 64-unit grid
        ctx.save();
        ctx.translate((width - 64 * s) / 2, (height - 64 * s) / 2);
        ctx.scale(s, s);
        ctx.fillStyle = colour;
        ctx.strokeStyle = colour;
        ctx.lineWidth = 6;
        ctx.lineJoin = "round";
        ctx.lineCap = "round";
        switch (kind) {
        case "tsuba": {
            // Guard disc with the blade slot and two side openings cut out (even-odd fill).
            ctx.beginPath();
            ctx.arc(32, 32, 30, 0, Math.PI * 2);
            ctx.moveTo(32, 14);
            ctx.bezierCurveTo(36, 20, 37, 26, 37, 32);
            ctx.bezierCurveTo(37, 38, 36, 44, 32, 50);
            ctx.bezierCurveTo(28, 44, 27, 38, 27, 32);
            ctx.bezierCurveTo(27, 26, 28, 20, 32, 14);
            ctx.moveTo(12, 24);
            ctx.bezierCurveTo(17, 24, 20, 28, 20, 32);
            ctx.bezierCurveTo(20, 36, 17, 40, 12, 40);
            ctx.bezierCurveTo(14, 37, 15, 34.5, 15, 32);
            ctx.bezierCurveTo(15, 29.5, 14, 27, 12, 24);
            ctx.moveTo(52, 24);
            ctx.bezierCurveTo(47, 24, 44, 28, 44, 32);
            ctx.bezierCurveTo(44, 36, 47, 40, 52, 40);
            ctx.bezierCurveTo(50, 37, 49, 34.5, 49, 32);
            ctx.bezierCurveTo(49, 29.5, 50, 27, 52, 24);
            ctx.fill("evenodd");
            break;
        }
        case "macro": {
            // Keycap outline with a play arrow.
            ctx.beginPath();
            ctx.roundedRect(8, 10, 48, 44, 9, 9);
            ctx.stroke();
            ctx.beginPath();
            ctx.moveTo(26, 21);
            ctx.lineTo(42, 32);
            ctx.lineTo(26, 43);
            ctx.closePath();
            ctx.fill();
            break;
        }
        case "links": {
            // Gamepad outline: body, d-pad and two buttons.
            ctx.beginPath();
            ctx.moveTo(18, 18);
            ctx.lineTo(46, 18);
            ctx.bezierCurveTo(56, 18, 60, 30, 60, 40);
            ctx.bezierCurveTo(60, 50, 52, 52, 48, 46);
            ctx.lineTo(43, 40);
            ctx.lineTo(21, 40);
            ctx.lineTo(16, 46);
            ctx.bezierCurveTo(12, 52, 4, 50, 4, 40);
            ctx.bezierCurveTo(4, 30, 8, 18, 18, 18);
            ctx.closePath();
            ctx.stroke();
            ctx.lineWidth = 5;
            ctx.beginPath();
            ctx.moveTo(19, 24);
            ctx.lineTo(19, 34);
            ctx.moveTo(14, 29);
            ctx.lineTo(24, 29);
            ctx.stroke();
            ctx.beginPath();
            ctx.arc(42, 26, 3.5, 0, Math.PI * 2);
            ctx.arc(49, 32, 3.5, 0, Math.PI * 2);
            ctx.fill();
            break;
        }
        case "gamelight": {
            // Game lighting: a compact gamepad in the lower half with three light rays above it.
            ctx.lineWidth = 5;
            ctx.beginPath();
            ctx.moveTo(19, 33);
            ctx.lineTo(45, 33);
            ctx.bezierCurveTo(54, 33, 58, 43, 58, 51);
            ctx.bezierCurveTo(58, 59, 51, 61, 47, 56);
            ctx.lineTo(43, 51);
            ctx.lineTo(21, 51);
            ctx.lineTo(17, 56);
            ctx.bezierCurveTo(13, 61, 6, 59, 6, 51);
            ctx.bezierCurveTo(6, 43, 10, 33, 19, 33);
            ctx.closePath();
            ctx.stroke();
            ctx.lineWidth = 4;
            ctx.beginPath();
            ctx.moveTo(20, 38);
            ctx.lineTo(20, 46);
            ctx.moveTo(16, 42);
            ctx.lineTo(24, 42);
            ctx.stroke();
            ctx.beginPath();
            ctx.arc(42, 40, 3, 0, Math.PI * 2);
            ctx.arc(48, 45, 3, 0, Math.PI * 2);
            ctx.fill();
            // Rays fanning upwards from the pad.
            ctx.lineWidth = 5;
            ctx.beginPath();
            ctx.moveTo(32, 25);
            ctx.lineTo(32, 6);
            ctx.moveTo(22, 26);
            ctx.lineTo(13, 12);
            ctx.moveTo(42, 26);
            ctx.lineTo(51, 12);
            ctx.stroke();
            break;
        }
        case "devices": {
            // A list: three rows, each a dot and a line.
            ctx.lineWidth = 5;
            for (let i = 0; i < 3; i++) {
                const y = 16 + i * 16;
                ctx.beginPath();
                ctx.arc(12, y, 4, 0, Math.PI * 2);
                ctx.fill();
                ctx.beginPath();
                ctx.moveTo(24, y);
                ctx.lineTo(56, y);
                ctx.stroke();
            }
            break;
        }
        case "studio": {
            // Prism: a light beam enters a triangle and fans out.
            ctx.beginPath();
            ctx.moveTo(30, 10);
            ctx.lineTo(50, 50);
            ctx.lineTo(10, 50);
            ctx.closePath();
            ctx.stroke();
            ctx.lineWidth = 4;
            ctx.beginPath();
            ctx.moveTo(2, 34);
            ctx.lineTo(22, 30);
            ctx.moveTo(38, 28);
            ctx.lineTo(62, 20);
            ctx.moveTo(40, 32);
            ctx.lineTo(62, 31);
            ctx.moveTo(42, 36);
            ctx.lineTo(62, 42);
            ctx.stroke();
            break;
        }
        }
        ctx.restore();
    }
}
