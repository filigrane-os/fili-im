// A video page (YouTube, Instagram, TikTok, Vimeo… whatever yt-dlp knows) played in place: the stream is looked up when
// it starts, then read by the player; if it cannot be, mpv is offered. Used inline in a bubble and full size in Quick Look.
import QtQuick
import QtMultimedia
import Fili.Im

Item {
    id: v
    property string url: ""
    property string poster: ""         // picture shown until it plays
    property bool compact: false       // in a bubble: a button to open Quick Look
    property bool autoplay: false
    property string phase: "idle"      // idle | resolving | ready | error
    property string error: ""
    property int rid: 0
    signal expand()
    readonly property bool playing: player.playbackState === MediaPlayer.PlayingState

    function start() {
        if (phase === "resolving") return;
        phase = "resolving";
        error = "";
        rid = Conversations.resolveVideo(url);
    }
    function toggle() {
        if (phase === "idle" || phase === "error") start();
        else if (playing) player.pause(); else player.play();
    }
    function stop() { player.stop(); player.source = ""; phase = "idle"; }
    onUrlChanged: { player.stop(); player.source = ""; phase = "idle"; if (autoplay && url !== "") start(); }
    Component.onCompleted: if (autoplay && url !== "") start()
    // Full-size use (Quick Look): plays when it becomes visible, stops when it is closed
    onAutoplayChanged: { if (autoplay && url !== "" && phase === "idle") start(); else if (!autoplay && !compact) stop(); }

    Connections {
        target: Conversations
        function onVideoResolved(id, json) {
            if (id !== v.rid) return;
            let r = {};
            try { r = JSON.parse(json); } catch (e) { }
            if (r.stream) { player.source = r.stream; v.phase = "ready"; player.play(); }
            else { v.phase = "error"; v.error = r.error || ""; }
        }
    }
    MediaPlayer {
        id: player
        videoOutput: out
        audioOutput: AudioOutput { }
        onErrorOccurred: (e, text) => { v.phase = "error"; v.error = text; }
    }

    Rectangle { anchors.fill: parent; color: "black"; radius: v.compact ? FiliTheme.radiusControl : 0 }
    Image {
        anchors.fill: parent
        visible: v.poster !== "" && v.phase !== "ready"
        source: visible ? v.poster : ""
        fillMode: v.compact ? Image.PreserveAspectCrop : Image.PreserveAspectFit
        asynchronous: true
    }
    VideoOutput { id: out; anchors.fill: parent; visible: v.phase === "ready" }

    // Play button, loading and failure
    Rectangle {
        visible: v.phase === "idle" || (v.phase === "ready" && !v.playing && player.position === 0)
        anchors.centerIn: parent
        width: v.compact ? 44 : 64
        height: width
        radius: width / 2
        color: FiliTheme.alpha(FiliTheme.backgroundDeep, 0.65)
        Text { anchors.centerIn: parent; text: "▶"; font.pixelSize: parent.width * 0.4; color: FiliTheme.text }
    }
    Text {
        visible: v.phase === "resolving"
        anchors.centerIn: parent
        text: Conversations.tr("Loading the video…")
        font.family: FiliTheme.font
        font.pixelSize: FiliTheme.sizeList
        color: "white"
        style: Text.Outline
        styleColor: "black"
    }
    Column {
        visible: v.phase === "error"
        anchors.centerIn: parent
        spacing: FiliTheme.space2
        width: parent.width - 2 * FiliTheme.space3
        Text {
            width: parent.width
            horizontalAlignment: Text.AlignHCenter
            text: Conversations.tr("This video cannot be played here") + (v.error !== "" ? " (" + v.error + ")" : "")
            wrapMode: Text.Wrap
            textFormat: Text.PlainText
            font.family: FiliTheme.font
            font.pixelSize: FiliTheme.sizeList
            color: "white"
            style: Text.Outline
            styleColor: "black"
        }
        TextButton {
            anchors.horizontalCenter: parent.horizontalCenter
            label: Conversations.tr("Play in mpv")
            primary: true
            onClicked: { if (!Conversations.playLink(v.url)) Conversations.openUrl(v.url); }
        }
    }
    MouseArea {
        anchors.fill: parent
        enabled: v.phase !== "error"
        cursorShape: Qt.PointingHandCursor
        onClicked: v.toggle()
    }
    // Controls: progress while it plays; in a bubble, a button to open Quick Look
    Rectangle {
        visible: v.phase === "ready"
        anchors { left: parent.left; right: parent.right; bottom: parent.bottom }
        height: 24
        color: Qt.rgba(0, 0, 0, 0.45)
        Rectangle {
            id: bar
            anchors { left: parent.left; leftMargin: FiliTheme.space2; right: parent.right; rightMargin: FiliTheme.space2; verticalCenter: parent.verticalCenter }
            height: 3
            radius: 1.5
            color: Qt.rgba(1, 1, 1, 0.3)
            Rectangle { width: player.duration > 0 ? parent.width * player.position / player.duration : 0; height: parent.height; radius: 1.5; color: FiliTheme.accentColor }
            MouseArea {
                anchors { fill: parent; topMargin: -8; bottomMargin: -8 }
                onPressed: mouse => player.position = Math.max(0, Math.min(1, mouse.x / width)) * player.duration
                onPositionChanged: mouse => { if (pressed) player.position = Math.max(0, Math.min(1, mouse.x / width)) * player.duration; }
            }
        }
    }
    IconButton {
        visible: v.compact && v.phase !== "idle"
        anchors { top: parent.top; right: parent.right; margins: FiliTheme.space1 }
        name: "maximize"
        tip: Conversations.tr("Open in Quick Look")
        onClicked: { player.pause(); v.expand(); }
    }
}
