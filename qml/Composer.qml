// The composer: Enter sends, Shift+Enter makes a new line, Up edits the last message (when the platform allows it),
// Esc drops the reply or the edit.
import QtQuick
import QtQuick.Controls.Basic
import QtQuick.Dialogs
import Fili.Im

Rectangle {
    id: c
    property var chat
    property var caps
    property var lastMine: null
    property var editing: null
    // Files waiting to go with the next message: [{path, name}]
    property var files: []
    color: FiliTheme.background
    height: bar.height + chips.height + input.height + 2 * FiliTheme.space2 + FiliTheme.borderWidth
    Rectangle { anchors { left: parent.left; right: parent.right; top: parent.top } height: FiliTheme.borderWidth; color: FiliTheme.border }

    function focusInput() { input.forceActiveFocus(); }
    function startEdit(m) { editing = m; win.replyTo = null; input.text = m.text.plain; input.forceActiveFocus(); input.cursorPosition = input.length; }
    function cancel() { editing = null; win.replyTo = null; }
    function addFiles(urls) {
        const next = files.slice();
        for (const u of urls) {
            const path = decodeURIComponent(String(u).replace(/^file:\/\//, ""));
            if (path !== "" && !next.some(f => f.path === path)) next.push({ path: path, name: path.split("/").pop() });
        }
        files = next;
        input.forceActiveFocus();
    }
    function submit() {
        const text = input.text.trim();
        if ((text === "" && files.length === 0) || !chat) return;
        if (editing) win.rpc("edit", { message: editing.id, text: text });
        else if (files.length > 0) {
            // One message per file; the text and the reply go with the first
            files.forEach((f, i) => win.rpc("send", { chat: chat.id, text: i === 0 ? text : "", media: { path: f.path }, reply_to: i === 0 && win.replyTo ? win.replyTo.native_id : undefined }));
            files = [];
        } else win.rpc("send", { chat: chat.id, text: text, reply_to: win.replyTo ? win.replyTo.native_id : undefined });
        input.text = "";
        cancel();
    }
    Connections { target: win; function onCurrentChanged() { c.cancel(); c.files = []; input.text = ""; } }

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
    FileDialog {
        id: picker
        title: Conversations.tr("Send files")
        fileMode: FileDialog.OpenFiles
        onAccepted: c.addFiles(selectedFiles)
    }
    Shortcut { sequence: "Ctrl+Shift+U"; enabled: c.visible; onActivated: picker.open() }
    DropArea {
        anchors.fill: parent
        keys: ["text/uri-list"]
        onDropped: drop => { if (drop.hasUrls) c.addFiles(drop.urls); }
    }
    // Files to send, one chip each (click removes it)
    Flow {
        id: chips
        visible: c.files.length > 0
        height: visible ? implicitHeight + FiliTheme.space2 : 0
        anchors { left: parent.left; right: parent.right; top: bar.bottom; leftMargin: FiliTheme.margin; rightMargin: FiliTheme.margin; topMargin: FiliTheme.space2 }
        spacing: FiliTheme.space2
        Repeater {
            model: c.files
            Rectangle {
                required property var modelData
                width: Math.min(chipLabel.implicitWidth + 2 * FiliTheme.space3 + 14, chips.width)
                height: 26
                radius: FiliTheme.radiusControl
                color: chipArea.containsMouse ? FiliTheme.surfaceHover : FiliTheme.backgroundDeep
                border.width: FiliTheme.borderWidth
                border.color: FiliTheme.border
                Text {
                    id: chipLabel
                    anchors { left: parent.left; leftMargin: FiliTheme.space3; right: chipX.left; rightMargin: FiliTheme.space1; verticalCenter: parent.verticalCenter }
                    text: modelData.name
                    elide: Text.ElideMiddle
                    textFormat: Text.PlainText
                    font.family: FiliTheme.font
                    font.pixelSize: FiliTheme.sizeList
                    color: FiliTheme.text
                }
                Text { id: chipX; anchors { right: parent.right; rightMargin: FiliTheme.space2; verticalCenter: parent.verticalCenter } text: "×"; font.pixelSize: FiliTheme.size; color: FiliTheme.muted }
                MouseArea { id: chipArea; anchors.fill: parent; hoverEnabled: true; onClicked: c.files = c.files.filter(f => f.path !== modelData.path) }
            }
        }
    }
    IconButton {
        id: attach
        anchors { left: parent.left; leftMargin: FiliTheme.margin - FiliTheme.space1; bottom: parent.bottom; bottomMargin: FiliTheme.space2 + 4 }
        name: "paperclip"
        tip: Conversations.tr("Send files…")
        enabledState: c.editing === null
        onClicked: picker.open()
    }
    Rectangle {
        id: field
        anchors { left: attach.right; right: parent.right; bottom: parent.bottom; leftMargin: FiliTheme.space1; rightMargin: FiliTheme.margin; bottomMargin: FiliTheme.space2 }
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
            if (e.key === Qt.Key_V && (e.modifiers & Qt.ControlModifier)) {
                const pasted = JSON.parse(Conversations.clipboardFiles());
                if (pasted.length > 0) { c.addFiles(pasted); e.accepted = true; return; }
            }
            if ((e.key === Qt.Key_Return || e.key === Qt.Key_Enter) && !(e.modifiers & Qt.ShiftModifier)) { c.submit(); e.accepted = true; }
            else if (e.key === Qt.Key_Escape) { c.cancel(); e.accepted = true; }
            else if (e.key === Qt.Key_Up && text === "" && c.lastMine && c.caps && c.caps.edit) { c.startEdit(c.lastMine); e.accepted = true; }
        }
    }
}
