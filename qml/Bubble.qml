// One message: sender (groups), quoted message, text (plain, selectable), media chip, reactions, time and state.
import QtQuick
import Fili.Im

Rectangle {
    id: b
    property var message
    property var quoted: null
    property bool showSender: false
    property real maxWidth: 480
    signal menu(real x, real y)
    signal retry()
    readonly property bool mine: message.from_me
    readonly property bool failed: message.state === "failed"
    readonly property string time: Qt.formatTime(new Date(message.timestamp), "hh:mm")
    readonly property string stateWord: message.state === "pending" ? Conversations.tr("sending") : failed ? Conversations.tr("not sent") : ""
    readonly property string mediaLabel: {
        const m = message.media;
        if (!m) return "";
        const kinds = { image: "Photo", video: "Video", audio: "Voice message", voice: "Voice message", file: "Document", document: "Document", sticker: "Sticker" };
        return (kinds[m.kind] ? Conversations.tr(kinds[m.kind]) : m.kind) + (m.name ? ": " + m.name : "");
    }
    readonly property string body: message.deleted ? Conversations.tr("Message deleted") : message.text.plain
    // Pictures show in the bubble (downloaded when it appears); videos and files download when clicked, then open
    readonly property bool hasMedia: message.media !== null && message.media !== undefined && !message.deleted
    readonly property bool isPicture: hasMedia && (message.media.kind === "image" || message.media.kind === "sticker")
    readonly property real pictureWidth: Math.min(280, b.inner)
    property string mediaPath: ""
    property bool fetching: false
    function fetchMedia(thenOpen) {
        if (fetching) return;
        if (mediaPath !== "") { if (thenOpen) Conversations.openUrl("file://" + encodeURI(mediaPath)); return; }
        fetching = true;
        win.rpc("media.fetch", { message: message.id }, r => {
            fetching = false;
            mediaPath = r.path;
            if (thenOpen) Conversations.openUrl("file://" + encodeURI(r.path));
        }, e => {
            fetching = false;
            win.notice = (e && e.message) || Conversations.tr("The file could not be downloaded");
        });
    }
    function sizeText(n) {
        if (!n) return "";
        if (n < 1024) return n + " B";
        if (n < 1048576) return Math.round(n / 1024) + " KB";
        return (n / 1048576).toFixed(1) + " MB";
    }
    Component.onCompleted: if (isPicture && (message.media.size || 0) < 15000000) fetchMedia(false)

    // The bubble is as wide as its widest line (text measured unwrapped), up to maxWidth; everything inside follows it
    readonly property real innerWidth: isPicture ? pictureWidth : b.body.indexOf("\n") >= 0 ? b.inner : Math.min(b.inner, Math.max(probe.width, probeSender.width, b.hasMedia && !b.isPicture ? 200 : 0, foot.implicitWidth, 60) + 2)
    width: innerWidth + 2 * FiliTheme.space3
    // The bubble is as wide as its widest single-line text (measured, capped at the maximum); text with line breaks
    // takes the maximum
    readonly property real inner: maxWidth - 2 * FiliTheme.space3
    TextMetrics { id: probe; elide: Qt.ElideRight; elideWidth: b.inner; text: b.body.indexOf("\n") >= 0 ? "" : b.body; font.family: FiliTheme.font; font.pixelSize: FiliTheme.size }
    TextMetrics { id: probeSender; elide: Qt.ElideRight; elideWidth: b.inner; text: b.showSender ? (b.message.sender_name || b.message.sender || "") : ""; font.family: FiliTheme.font; font.pixelSize: FiliTheme.sizeList; font.weight: FiliTheme.weightAccent }
    TextMetrics { id: probeMedia; elide: Qt.ElideRight; elideWidth: b.inner; text: b.hasMedia ? b.mediaLabel : ""; font.family: FiliTheme.font; font.pixelSize: FiliTheme.sizeList }
    height: content.height + 2 * FiliTheme.space2
    radius: FiliTheme.radiusWindow
    color: mine ? FiliTheme.surfaceActive : FiliTheme.surface
    border.width: failed ? FiliTheme.borderWidth : 0
    border.color: FiliTheme.danger

    Column {
        id: content
        x: FiliTheme.space3
        y: FiliTheme.space2
        width: b.width - 2 * FiliTheme.space3
        spacing: 2
        Text {
            id: senderLabel
            visible: b.showSender
            width: parent.width
            text: b.message.sender_name || b.message.sender || ""
            elide: Text.ElideRight
            textFormat: Text.PlainText
            font.family: FiliTheme.font
            font.pixelSize: FiliTheme.sizeList
            font.weight: FiliTheme.weightAccent
            color: FiliTheme.accentColor
        }
        Rectangle {
            visible: b.quoted !== null
            width: parent.width
            height: visible ? quote.implicitHeight + FiliTheme.space2 : 0
            color: FiliTheme.alpha(FiliTheme.background, 0.4)
            radius: FiliTheme.radiusControl
            Rectangle { width: 2; height: parent.height; color: FiliTheme.accentColor }
            Text {
                id: quote
                x: FiliTheme.space2
                y: FiliTheme.space1 / 2
                width: parent.width - 2 * FiliTheme.space2
                text: b.quoted ? (b.quoted.from_me ? Conversations.tr("You") : (b.quoted.sender_name || "")) + ": " + (b.quoted.text.plain || "…") : ""
                maximumLineCount: 2
                elide: Text.ElideRight
                wrapMode: Text.Wrap
                textFormat: Text.PlainText
                font.family: FiliTheme.font
                font.pixelSize: FiliTheme.sizeSmall
                color: FiliTheme.textSecondary
            }
        }
        // Picture: shown once downloaded, a placeholder of the same width before
        Rectangle {
            id: picture
            visible: b.isPicture
            width: parent.width
            height: visible ? (image.status === Image.Ready && image.implicitWidth > 0 ? Math.min(360, width * image.implicitHeight / image.implicitWidth) : 120) : 0
            radius: FiliTheme.radiusControl
            color: FiliTheme.backgroundDeep
            clip: true
            Image {
                id: image
                anchors.fill: parent
                source: b.mediaPath !== "" ? "file://" + encodeURI(b.mediaPath) : ""
                asynchronous: true
                cache: true
                sourceSize.width: 640
                fillMode: Image.PreserveAspectFit
            }
            Text {
                anchors.centerIn: parent
                visible: image.status !== Image.Ready
                text: b.fetching ? Conversations.tr("Downloading…") : (b.mediaPath === "" ? "▣" : "")
                font.family: FiliTheme.font
                font.pixelSize: FiliTheme.sizeList
                color: FiliTheme.muted
            }
            MouseArea { anchors.fill: parent; cursorShape: Qt.PointingHandCursor; onClicked: b.mediaPath === "" ? b.fetchMedia(true) : Conversations.openUrl("file://" + encodeURI(b.mediaPath)) }
        }
        // Video, document, voice message: a chip (click downloads and opens it)
        Rectangle {
            id: chip
            visible: b.hasMedia && !b.isPicture
            width: parent.width
            height: visible ? 40 : 0
            radius: FiliTheme.radiusControl
            color: chipArea.containsMouse ? FiliTheme.surfaceHover : FiliTheme.alpha(FiliTheme.background, 0.4)
            Text {
                id: chipGlyph
                anchors { left: parent.left; leftMargin: FiliTheme.space2; verticalCenter: parent.verticalCenter }
                text: b.hasMedia && b.message.media.kind === "video" ? "▶" : b.hasMedia && (b.message.media.kind === "audio" || b.message.media.kind === "voice") ? "♪" : "▣"
                font.pixelSize: FiliTheme.size + 4
                color: FiliTheme.accentColor
            }
            Column {
                anchors { left: chipGlyph.right; leftMargin: FiliTheme.space2; right: parent.right; rightMargin: FiliTheme.space2; verticalCenter: parent.verticalCenter }
                Text {
                    width: parent.width
                    text: b.mediaLabel
                    elide: Text.ElideMiddle
                    textFormat: Text.PlainText
                    font.family: FiliTheme.font
                    font.pixelSize: FiliTheme.sizeList
                    color: FiliTheme.text
                }
                Text {
                    width: parent.width
                    text: b.fetching ? Conversations.tr("Downloading…") : b.sizeText(b.hasMedia ? b.message.media.size : 0)
                    elide: Text.ElideRight
                    font.family: FiliTheme.font
                    font.pixelSize: FiliTheme.sizeSmall
                    color: FiliTheme.muted
                }
            }
            MouseArea { id: chipArea; anchors.fill: parent; hoverEnabled: true; cursorShape: Qt.PointingHandCursor; onClicked: b.fetchMedia(true) }
        }
        TextEdit {
            visible: b.body !== ""
            width: parent.width
            text: b.body
            readOnly: true
            selectByMouse: true
            wrapMode: TextEdit.Wrap
            textFormat: TextEdit.PlainText
            selectionColor: FiliTheme.surfaceHover
            selectedTextColor: FiliTheme.text
            font.family: FiliTheme.font
            font.pixelSize: FiliTheme.size
            font.italic: b.message.deleted
            color: b.message.deleted ? FiliTheme.muted : FiliTheme.text
        }
        Row {
            visible: b.message.reactions && b.message.reactions.length > 0
            spacing: FiliTheme.space1
            Repeater {
                model: b.message.reactions || []
                Text { required property var modelData; text: modelData.emoji; font.pixelSize: FiliTheme.size }
            }
        }
        Row {
            id: foot
            x: content.width - width
            spacing: FiliTheme.space2
            Text {
                visible: b.failed
                text: Conversations.tr("Send again")
                font.family: FiliTheme.font
                font.pixelSize: FiliTheme.sizeSmall
                color: FiliTheme.accentColor
                MouseArea { anchors.fill: parent; onClicked: b.retry() }
            }
            Text {
                text: (b.message.edited ? Conversations.tr("edited") + " · " : "") + (b.stateWord !== "" ? b.stateWord + " · " : "") + b.time
                font.family: FiliTheme.font
                font.pixelSize: FiliTheme.sizeSmall
                color: b.failed ? FiliTheme.danger : FiliTheme.muted
            }
        }
    }
    // Right click anywhere on the bubble (the text selection would swallow it in a MouseArea)
    TapHandler {
        acceptedButtons: Qt.RightButton
        onTapped: (point) => b.menu(point.position.x, point.position.y)
    }
}
