// Endless loading indicator: a turning arrow, for what is being fetched.
import QtQuick
import Fili.Im

Item {
    id: s
    property int size: 32
    property color color: FiliTheme.accentColor
    width: size
    height: size
    Icon {
        anchors.centerIn: parent
        name: "refresh"
        size: s.size
        color: s.color
        RotationAnimator on rotation { from: 0; to: 360; duration: 1100; loops: Animation.Infinite; running: s.visible }
    }
}
