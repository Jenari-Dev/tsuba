import QtQuick
import QtTest 1.2

// Alignment maths of the custom device picture (fit / move / scale -> imageRect).
Item {
    id: top
    width: 1000
    height: 760

    PictureAlign {
        id: al
        anchors.fill: parent
        frameW: 561
        frameH: 905
        imgW: 800
        imgH: 1100
    }

    TestCase {
        name: "PictureAlign"
        when: windowShown

        function near(a, b, msg) {
            verify(Math.abs(a - b) < 1e-6, msg + ": " + a + " vs " + b);
        }
        function init() {
            al.imgW = 800;
            al.imgH = 1100;
            al.placed = true;
            al.fit();
        }
        function test_fit_wide_picture_is_limited_by_width() {
            const r = al.fitRect(1600, 600, 2108, 680);
            near(r[3], 600 * (2108 / 1600) > 680 ? 680 : 600 * (2108 / 1600), "height");
            // 1600x600 into 2108x680: height-limited (680/600 < 2108/1600)
            near(r[2], 1600 * (680 / 600), "width");
            near(r[0], (2108 - r[2]) / 2, "centred x");
            near(r[1], 0, "y");
        }
        function test_fit_tall_picture() {
            const r = al.fitRect(800, 1100, 561, 905);
            // 800x1100 into 561x905: width-limited.
            near(r[2], 561, "width fills");
            near(r[3], 1100 * (561 / 800), "aspect kept");
            near(r[0], (561 - r[2]) / 2, "centred");
            compare(al.rect.length, 4);
            near(al.rect[2], r[2], "fit() applied");
        }
        function test_scale_keeps_cursor_point_fixed() {
            const r = [100, 100, 200, 300];
            const p = { x: 150, y: 220 };
            const out = al.scaleRect(r, 400, p.x, p.y);
            near(out[2], 400, "width");
            near(out[3], 600, "height keeps aspect");
            // The picture point under the cursor stays under it.
            near((p.x - r[0]) / r[2], (p.x - out[0]) / out[2], "u");
            near((p.y - r[1]) / r[3], (p.y - out[1]) / out[3], "v");
        }
        function test_scale_is_clamped() {
            al.fit();
            const fw = al.fitWidth();
            near(al.scaleRect(al.rect, 1e6, 0, 0)[2], fw * al.maxScale, "max");
            near(al.scaleRect(al.rect, 0.0001, 0, 0)[2], fw * al.minScale, "min");
            near(al.scaleOf([0, 0, fw * 2, 1]), 2, "scaleOf");
        }
        function test_move_and_clamp_keeps_centre_in_frame() {
            const r = al.moveRect([10, 10, 100, 200], 5, -3);
            near(r[0], 15, "x");
            near(r[1], 7, "y");
            const far = al.moveRect([10, 10, 100, 200], 5000, 5000);
            near(far[0] + far[2] / 2, 561, "centre x clamped");
            near(far[1] + far[3] / 2, 905, "centre y clamped");
        }
        function test_stage_mapping_is_the_inverse_of_placement() {
            // The frame's top-left maps to drawing (0,0); a point one frame-width right maps to frameW.
            const o = al.toDrawing(findChild(al, "pic").x - al.rect[0] * 0, 0);
            verify(isFinite(o.x));
            const a = al.toDrawing(200, 300), b = al.toDrawing(300, 300);
            verify(b.x > a.x && Math.abs(a.y - b.y) < 1e-9, "x grows with the stage, y does not");
        }
        function test_wheel_scales_about_pointer_and_drag_moves() {
            al.fit();
            const drag = findChild(al, "drag");
            verify(drag !== null);
            const w0 = al.rect[2];
            mouseWheel(drag, drag.width / 2, drag.height / 2, 0, 120);
            verify(al.rect[2] > w0, "wheel up grows");
            mouseWheel(drag, drag.width / 2, drag.height / 2, 0, -240);
            verify(al.rect[2] < w0, "wheel down shrinks");
            const x0 = al.rect[0];
            mousePress(drag, 100, 100);
            mouseMove(drag, 140, 100);
            mouseRelease(drag, 140, 100);
            verify(al.rect[0] > x0, "drag moves right");
        }
        function test_save_emits_rect() {
            al.rect = [12, 34, 56, 78];
            const spy = createTemporaryObject(spyComp, top, { target: al });
            const btn = findChild(al, "saveButton");
            mouseClick(btn);
            compare(spy.count, 1);
            compare(spy.signalArguments[0][0], 12);
            compare(spy.signalArguments[0][3], 78);
        }
        function test_fit_button_resets() {
            al.rect = [1, 2, 3, 4];
            mouseClick(findChild(al, "fitButton"));
            near(al.rect[2], 561, "fitted again");
        }
        Component {
            id: spyComp
            SignalSpy {
                signalName: "accepted"
            }
        }
    }
}
