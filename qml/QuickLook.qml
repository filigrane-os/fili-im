// Quick Look: the pictures, videos, voice messages and documents of the conversation in one overlay.
// Left/Right go to the previous/next one (the next ones are downloaded ahead), Space plays or pauses a video
// (closes for the others), Esc closes. Files come from the daemon (`media.fetch`) when first shown.
import QtQuick
import QtMultimedia
import Fili.Im

Rectangle {
    id: ql
    z: 220
    visible: false
    color: FiliTheme.alpha(FiliTheme.backgroundDeep, 0.92)
    focus: visible

    property var items: []
    property int index: 0
    property var paths: ({})
    property var failed: ({})
    property var inflight: ({})
    readonly property var current: items.length > 0 ? items[index] : null
    readonly property string kind: current && current.media ? current.media.kind : ""
    readonly property string path: current ? (paths[current.id] || "") : ""
    readonly property bool isPicture: kind === "image" || kind === "sticker"
    readonly property bool isVideo: kind === "video"
    readonly property bool isAudio: kind === "audio" || kind === "voice"
    readonly property bool playable: isVideo || isAudio

    // The media of `messages`, oldest first; opens on the message `id`
    function open(messages, id) {
        items = messages.filter(m => m.media && !m.deleted);
        const i = items.findIndex(m => m.id === id);
        if (i < 0) return;
        index = i;
        visible = true;
        forceActiveFocus();
        ensure(index);
    }
    function close() { player.stop(); visible = false; }
    function go(step) {
        const i = index + step;
        if (i < 0 || i >= items.length) return;
        player.stop();
        index = i;
        ensure(i);
    }
    function ensure(i) {
        for (const k of [i, i + 1, i - 1]) {
            if (k < 0 || k >= items.length) continue;
            const m = items[k];
            if (paths[m.id] || failed[m.id] || (k !== i && (m.media.size || 0) > 30000000)) continue;
            fetch(m);
        }
    }
    function fetch(m) {
        if (inflight[m.id]) return;
        inflight[m.id] = true;
        win.rpc("media.fetch", { message: m.id }, r => {
            const p = Object.assign({}, paths); p[m.id] = r.path; paths = p;
        }, e => {
            const f = Object.assign({}, failed); f[m.id] = (e && e.message) || Conversations.tr("The file could not be downloaded"); failed = f;
        });
    }
    function sizeText(n) {
        if (!n) return "";
        if (n < 1024) return n + " B";
        if (n < 1048576) return Math.round(n / 1024) + " KB";
        return (n / 1048576).toFixed(1) + " MB";
    }
    function clock(ms) {
        const s = Math.max(0, Math.floor(ms / 1000));
        return Math.floor(s / 60) + ":" + String(s % 60).padStart(2, "0");
    }
    function togglePlay() {
        if (player.playbackState === MediaPlayer.PlayingState) player.pause(); else player.play();
    }
    onPathChanged: {
        player.stop();
        player.source = playable && path !== "" ? "file://" + encodeURI(path) : "";
        if (playable && path !== "" && visible) player.play();
    }
    onKindChanged: if (!playable) player.source = ""

    Keys.onEscapePressed: close()
    Keys.onLeftPressed: go(-1)
    Keys.onRightPressed: go(1)
    Keys.onSpacePressed: playable && path !== "" ? togglePlay() : close()

    MediaPlayer {
        id: player
        videoOutput: video
        audioOutput: AudioOutput { }
    }

    // A click on the background closes it
    MouseArea { anchors.fill: parent; onClicked: ql.close() }

    // Title bar
    Item {
        id: top
        anchors { left: parent.left; right: parent.right; top: parent.top }
        height: 44
        Text {
            anchors { left: parent.left; leftMargin: FiliTheme.margin; right: tools.left; rightMargin: FiliTheme.space3; verticalCenter: parent.verticalCenter }
            text: ql.current ? ((ql.current.sender_name || (ql.current.from_me ? Conversations.tr("You") : "")) + (ql.current.media.name ? " · " + ql.current.media.name : "")) : ""
            elide: Text.ElideMiddle
            textFormat: Text.PlainText
            font.family: FiliTheme.font
            font.pixelSize: FiliTheme.size
            font.weight: FiliTheme.weightAccent
            color: FiliTheme.text
        }
        Row {
            id: tools
            anchors { right: parent.right; rightMargin: FiliTheme.margin; verticalCenter: parent.verticalCenter }
            spacing: FiliTheme.space2
            Text {
                anchors.verticalCenter: parent.verticalCenter
                text: ql.items.length > 1 ? (ql.index + 1) + " / " + ql.items.length : ""
                font.family: FiliTheme.font
                font.pixelSize: FiliTheme.sizeList
                color: FiliTheme.muted
            }
            IconButton { name: "forward"; tip: Conversations.tr("Forward"); onClicked: { const m = ql.current; ql.close(); win.forwardMessage(m); } }
            IconButton { name: "download"; tip: Conversations.tr("Open with the default application"); enabledState: ql.path !== ""; onClicked: Conversations.openUrl("file://" + encodeURI(ql.path)) }
            IconButton { name: "x"; tip: Conversations.tr("Close"); onClicked: ql.close() }
        }
    }

    // Content
    Item {
        id: stage
        anchors { left: parent.left; right: parent.right; top: top.bottom; bottom: bar.top; leftMargin: 64; rightMargin: 64 }
        MouseArea { anchors.fill: parent; onClicked: ql.playable && ql.path !== "" ? ql.togglePlay() : undefined }
        Image {
            anchors.fill: parent
            visible: ql.isPicture && ql.path !== ""
            source: ql.isPicture && ql.path !== "" ? "file://" + encodeURI(ql.path) : ""
            fillMode: Image.PreserveAspectFit
            asynchronous: true
            cache: false
            sourceSize.width: 2400
        }
        VideoOutput {
            id: video
            anchors.fill: parent
            visible: ql.isVideo && ql.path !== ""
        }
        // Voice message / audio / document: a card
        Column {
            anchors.centerIn: parent
            visible: (ql.isAudio || (!ql.isPicture && !ql.isVideo)) && ql.current !== null
            spacing: FiliTheme.space2
            Text {
                anchors.horizontalCenter: parent.horizontalCenter
                text: ql.isAudio ? "♪" : "▣"
                font.pixelSize: 64
                color: FiliTheme.accentColor
            }
            Text {
                anchors.horizontalCenter: parent.horizontalCenter
                text: ql.current && ql.current.media.name ? ql.current.media.name : (ql.current ? ql.current.media.kind : "")
                textFormat: Text.PlainText
                font.family: FiliTheme.font
                font.pixelSize: FiliTheme.size
                color: FiliTheme.text
            }
            Text {
                anchors.horizontalCenter: parent.horizontalCenter
                text: ql.current ? ql.sizeText(ql.current.media.size) : ""
                font.family: FiliTheme.font
                font.pixelSize: FiliTheme.sizeList
                color: FiliTheme.muted
            }
            TextButton {
                anchors.horizontalCenter: parent.horizontalCenter
                visible: !ql.isAudio && ql.path !== ""
                label: Conversations.tr("Open")
                onClicked: Conversations.openUrl("file://" + encodeURI(ql.path))
            }
        }
        // Waiting / failure
        Text {
            anchors.centerIn: parent
            visible: ql.current !== null && ql.path === "" && (ql.isPicture || ql.isVideo)
            text: ql.current && ql.failed[ql.current.id] ? ql.failed[ql.current.id] : Conversations.tr("Downloading…")
            font.family: FiliTheme.font
            font.pixelSize: FiliTheme.sizeList
            color: FiliTheme.muted
        }
    }

    // Arrows
    IconButton {
        anchors { left: parent.left; leftMargin: FiliTheme.space3; verticalCenter: stage.verticalCenter }
        name: "chevron-left"
        width: 40
        height: 40
        visible: ql.index > 0
        onClicked: ql.go(-1)
    }
    IconButton {
        anchors { right: parent.right; rightMargin: FiliTheme.space3; verticalCenter: stage.verticalCenter }
        name: "chevron-right"
        width: 40
        height: 40
        visible: ql.index < ql.items.length - 1
        onClicked: ql.go(1)
    }

    // Player bar (video and audio)
    Item {
        id: bar
        anchors { left: parent.left; right: parent.right; bottom: parent.bottom }
        height: ql.playable && ql.path !== "" ? 44 : 0
        visible: height > 0
        Row {
            anchors { left: parent.left; leftMargin: FiliTheme.margin; verticalCenter: parent.verticalCenter }
            id: playRow
            spacing: FiliTheme.space2
            Text {
                anchors.verticalCenter: parent.verticalCenter
                width: 20
                text: player.playbackState === MediaPlayer.PlayingState ? "❚❚" : "▶"
                font.pixelSize: FiliTheme.size
                color: FiliTheme.text
                MouseArea { anchors.fill: parent; onClicked: ql.togglePlay() }
            }
            Text {
                anchors.verticalCenter: parent.verticalCenter
                text: ql.clock(player.position)
                font.family: FiliTheme.font
                font.pixelSize: FiliTheme.sizeSmall
                color: FiliTheme.muted
            }
        }
        Text {
            id: total
            anchors { right: parent.right; rightMargin: FiliTheme.margin; verticalCenter: parent.verticalCenter }
            text: ql.clock(player.duration)
            font.family: FiliTheme.font
            font.pixelSize: FiliTheme.sizeSmall
            color: FiliTheme.muted
        }
        Rectangle {
            id: track
            anchors { left: playRow.right; leftMargin: FiliTheme.space3; right: total.left; rightMargin: FiliTheme.space3; verticalCenter: parent.verticalCenter }
            height: 4
            radius: 2
            color: FiliTheme.surface
            Rectangle {
                width: player.duration > 0 ? parent.width * player.position / player.duration : 0
                height: parent.height
                radius: 2
                color: FiliTheme.accentColor
            }
            MouseArea {
                anchors { fill: parent; topMargin: -10; bottomMargin: -10 }
                onPressed: mouse => player.position = Math.max(0, Math.min(1, mouse.x / width)) * player.duration
                onPositionChanged: mouse => { if (pressed) player.position = Math.max(0, Math.min(1, mouse.x / width)) * player.duration; }
            }
        }
    }
}
