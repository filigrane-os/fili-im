// The composer: Enter sends, Shift+Enter makes a new line, Up edits the last message (when the platform allows it),
// Esc drops the reply or the edit.
import QtQuick
import QtQuick.Controls.Basic
import Fili.Im

Rectangle {
    id: c
    property var chat
    property var caps
    property var lastMine: null
    property var editing: null
    color: FiliTheme.background
    height: bar.height + input.height + 2 * FiliTheme.space2 + FiliTheme.borderWidth
    Rectangle { anchors { left: parent.left; right: parent.right; top: parent.top } height: FiliTheme.borderWidth; color: FiliTheme.border }

    function focusInput() { input.forceActiveFocus(); }
    function startEdit(m) { editing = m; win.replyTo = null; input.text = m.text.plain; input.forceActiveFocus(); input.cursorPosition = input.length; }
    function cancel() { editing = null; win.replyTo = null; }
    function submit() {
        const text = input.text.trim();
        if (text === "" || !chat) return;
        if (editing) win.rpc("edit", { message: editing.id, text: text });
        else win.rpc("send", { chat: chat.id, text: text, reply_to: win.replyTo ? win.replyTo.native_id : undefined });
        input.text = "";
        cancel();
    }
    Connections { target: win; function onCurrentChanged() { c.cancel(); input.text = ""; } }

    Rectangle {
        id: bar
        visible: c.editing !== null || win.replyTo !== null
        anchors { left: parent.left; right: parent.right; top: parent.top; topMargin: FiliTheme.space2 }
        height: visible ? 24 : 0
        color: "transparent"
        Text {
            anchors { left: parent.left; leftMargin: FiliTheme.margin; right: dismiss.left; rightMargin: FiliTheme.space2; verticalCenter: parent.verticalCenter }
            text: c.editing ? Conversations.tr("Editing message") : (win.replyTo ? Conversations.tr("Replying to") + " " + (win.replyTo.from_me ? Conversations.tr("yourself") : (win.replyTo.sender_name || "")) + ": " + win.replyTo.text.plain : "")
            elide: Text.ElideRight
            textFormat: Text.PlainText
            font.family: FiliTheme.font
            font.pixelSize: FiliTheme.sizeSmall
            color: FiliTheme.textSecondary
        }
        IconButton { id: dismiss; anchors { right: parent.right; rightMargin: FiliTheme.margin - FiliTheme.space1; verticalCenter: parent.verticalCenter } name: "x"; tip: Conversations.tr("Cancel"); onClicked: c.cancel() }
    }
    Rectangle {
        id: field
        anchors { left: parent.left; right: parent.right; bottom: parent.bottom; leftMargin: FiliTheme.margin; rightMargin: FiliTheme.margin; bottomMargin: FiliTheme.space2 }
        height: input.height
        radius: FiliTheme.radiusControl
        color: FiliTheme.surface
        border.width: FiliTheme.borderWidth
        border.color: input.activeFocus ? FiliTheme.accentColor : FiliTheme.border
    }
    TextArea {
        id: input
        anchors { left: field.left; right: field.right; bottom: field.bottom }
        height: Math.min(140, Math.max(32, contentHeight + 2 * topPadding))
        leftPadding: FiliTheme.space2
        rightPadding: FiliTheme.space2
        topPadding: 7
        bottomPadding: 7
        wrapMode: TextArea.Wrap
        placeholderText: Conversations.tr("Message")
        placeholderTextColor: FiliTheme.muted
        font.family: FiliTheme.font
        font.pixelSize: FiliTheme.size
        color: FiliTheme.text
        selectionColor: FiliTheme.surfaceActive
        selectedTextColor: FiliTheme.text
        background: null
        Keys.onPressed: e => {
            if ((e.key === Qt.Key_Return || e.key === Qt.Key_Enter) && !(e.modifiers & Qt.ShiftModifier)) { c.submit(); e.accepted = true; }
            else if (e.key === Qt.Key_Escape) { c.cancel(); e.accepted = true; }
            else if (e.key === Qt.Key_Up && text === "" && c.lastMine && c.caps && c.caps.edit) { c.startEdit(c.lastMine); e.accepted = true; }
        }
    }
}
