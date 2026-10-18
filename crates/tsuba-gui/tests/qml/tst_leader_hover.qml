import QtQuick
import QtTest 1.2

// Leader lines light up in the accent while their label or button hotspot is hovered.
Item {
    id: top
    width: 1400
    height: 700

    property var pack: ({
        art: "tsuba",
        views: {
            top: {
                image: "x.svg", width: 561, height: 905,
                hotspots: [
                    { name: "Left Click", pos: 1, anchor: [250, 140], side: "right" },
                    { name: "Mouse Button 4", pos: 4, anchor: [177, 430], side: "left" }
                ]
            }
        }
    })

    MouseImageView {
        id: view
        anchors.fill: parent
        pack: top.pack
        dir: ""
        assignments: ({ 1: { function: 1, data: [1, 0] }, 4: { function: 1, data: [4, 0] } })
    }

    TestCase {
        name: "LeaderHover"
        when: windowShown

        function test_hover_on_button_hotspot() {
            compare(view.hoveredPos, -1);
            const p = view.anchorPt(top.pack.views.top.hotspots[0]);
            mouseMove(view, p.x, p.y);
            tryCompare(view, "hoveredPos", 1);
            mouseMove(view, 5, view.height - 5);
            tryCompare(view, "hoveredPos", -1);
        }
        function test_hover_on_label() {
            const s = view.rightSlots[0];
            mouseMove(view, view.width - 40, s.y + view.calloutH / 2);
            tryCompare(view, "hoveredPos", 1);
            mouseMove(view, view.width / 2, 2);
            tryCompare(view, "hoveredPos", -1);
        }
        function test_image_rect_places_the_picture() {
            // Without imageRect the picture covers the drawing; with it, it sits at the rect.
            compare(Theme.imageRect(top.pack.views.top)[2], 561);
            const v = { width: 561, height: 905, imageRect: [10, 20, 300, 400] };
            compare(Theme.imageRect(v)[0], 10);
            compare(Theme.imageRect(v)[3], 400);
            compare(Theme.imageRect({ width: 5, height: 6, imageRect: [1, 2, 0, 4] })[2], 5, "degenerate rect ignored");
        }
    }
}
