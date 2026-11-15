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
        const kinds = { image: "Photo", video: "Video", audio: "Voice message", document: "Document", sticker: "Sticker" };
        return (kinds[m.kind] ? Conversations.tr(kinds[m.kind]) : m.kind) + (m.name ? ": " + m.name : "");
    }
    readonly property string body: message.deleted ? Conversations.tr("Message deleted") : message.text.plain

    // The bubble is as wide as its widest line (text measured unwrapped), up to maxWidth; everything inside follows it
    readonly property real innerWidth: b.body.indexOf("\n") >= 0 ? b.inner : Math.min(b.inner, Math.max(probe.width, probeSender.width, probeMedia.width, foot.implicitWidth, 60) + 2)
    width: innerWidth + 2 * FiliTheme.space3
    // The bubble is as wide as its widest single-line text (measured, capped at the maximum); text with line breaks
    // takes the maximum
    readonly property real inner: maxWidth - 2 * FiliTheme.space3
    TextMetrics { id: probe; elide: Qt.ElideRight; elideWidth: b.inner; text: b.body.indexOf("\n") >= 0 ? "" : b.body; font.family: FiliTheme.font; font.pixelSize: FiliTheme.size }
    TextMetrics { id: probeSender; elide: Qt.ElideRight; elideWidth: b.inner; text: b.showSender ? (b.message.sender_name || b.message.sender || "") : ""; font.family: FiliTheme.font; font.pixelSize: FiliTheme.sizeList; font.weight: FiliTheme.weightAccent }
    TextMetrics { id: probeMedia; elide: Qt.ElideRight; elideWidth: b.inner; text: b.mediaLabel !== "" ? "▣ " + b.mediaLabel : ""; font.family: FiliTheme.font; font.pixelSize: FiliTheme.sizeList }
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
        Text {
            id: mediaLabelItem
            visible: b.mediaLabel !== ""
            text: "▣ " + b.mediaLabel
            width: parent.width
            elide: Text.ElideRight
            textFormat: Text.PlainText
            font.family: FiliTheme.font
            font.pixelSize: FiliTheme.sizeList
            color: FiliTheme.textSecondary
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
    MouseArea {
        anchors.fill: parent
        acceptedButtons: Qt.RightButton
        z: -1
        onClicked: mouse => b.menu(mouse.x, mouse.y)
    }
}
