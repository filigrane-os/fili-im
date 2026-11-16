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
    signal platformMenu(real x, real y)
    signal sent()
    // In the view of all the conversations of a person: which platform the message goes to
    readonly property bool choosing: win.grouped && win.scope === 0
    property var editing: null
    // Files waiting to go with the next message: [{path, name}]
    property var files: []
    color: FiliTheme.background
    height: bar.height + chips.height + input.height + 2 * FiliTheme.space2 + FiliTheme.borderWidth
    Rectangle { anchors { left: parent.left; right: parent.right; top: parent.top } height: FiliTheme.borderWidth; color: FiliTheme.border }

    // ---- Voice messages: the microphone is recorded to an Ogg Opus file and sent as a voice note --------------------
    property string voiceFile: ""
    property bool sendWhenStopped: false
    property bool recording: false
    property real startedAt: 0
    property int elapsed: 0
    function startRecording() {
        if (recording) return;
        voiceFile = Conversations.voicePath();
        if (!Conversations.startVoice(voiceFile)) { win.notice = Conversations.tr("The microphone cannot be recorded (pw-record and opusenc are needed)"); return; }
        startedAt = Date.now();
        elapsed = 0;
        recording = true;
    }
    function stopRecording(send) {
        if (!recording) return;
        sendWhenStopped = send;
        elapsed = Date.now() - startedAt;
        recording = false;
        Conversations.stopVoice();
    }
    function toggleRecording() { if (recording) stopRecording(true); else startRecording(); }
    Timer { interval: 250; repeat: true; running: c.recording; onTriggered: c.elapsed = Date.now() - c.startedAt }
    Connections {
        target: Conversations
        function onVoiceFinished(file, ok) {
            if (file !== c.voiceFile) return;
            const send = c.sendWhenStopped && ok && c.elapsed >= 700 && c.chat;
            c.sendWhenStopped = false;
            if (!send) { Conversations.discardVoice(file); return; }
            win.rpc("send", { chat: c.chat.id, text: "", media: { path: file, kind: "voice", mime: "audio/ogg; codecs=opus" }, reply_to: win.replyTo ? win.replyTo.native_id : undefined },
                () => Conversations.discardVoice(file), () => Conversations.discardVoice(file));
            c.cancel();
            c.sent();
        }
    }
    function clock(ms) { const s = Math.floor(ms / 1000); return Math.floor(s / 60) + ":" + String(s % 60).padStart(2, "0"); }
    Shortcut { sequence: "Ctrl+Shift+R"; enabled: c.visible && side.canVoice; onActivated: c.toggleRecording() }

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
        c.sent();
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
    // Where the message goes (only in the view of all the conversations of a person): inside the box, far right
    Rectangle {
        id: target
        visible: c.choosing && c.chat !== null && !c.recording
        z: 3
        anchors { right: field.right; rightMargin: FiliTheme.space1; bottom: field.bottom; bottomMargin: 4 }
        width: visible ? 46 : 0
        height: 24
        radius: FiliTheme.radiusControl
        color: targetArea.containsMouse ? FiliTheme.surfaceHover : "transparent"
        Rectangle {
            anchors { left: parent.left; leftMargin: FiliTheme.space2; verticalCenter: parent.verticalCenter }
            width: 16; height: 16; radius: 8
            color: c.chat ? win.platformColor(win.platformOf(c.chat)) : "transparent"
            Text { anchors.centerIn: parent; text: c.chat ? win.platformName(win.platformOf(c.chat)).charAt(0) : ""; font.pixelSize: 10; font.bold: true; color: "white" }
        }
        Text { anchors { right: parent.right; rightMargin: FiliTheme.space2; verticalCenter: parent.verticalCenter } text: "▾"; color: FiliTheme.muted }
        MouseArea { id: targetArea; anchors.fill: parent; hoverEnabled: true; cursorShape: Qt.PointingHandCursor; onClicked: { const p = target.mapToItem(c, 0, 0); c.platformMenu(p.x, p.y); } }
        ToolTip { visible: targetArea.containsMouse; delay: 600; text: Conversations.tr("Send on…") }
    }
    // Send (something is written) or record a voice message (nothing is)
    IconButton {
        id: side
        visible: !c.recording
        anchors { right: parent.right; rightMargin: FiliTheme.margin - FiliTheme.space1; bottom: parent.bottom; bottomMargin: FiliTheme.space2 + 4 }
        readonly property bool hasContent: input.text.trim() !== "" || c.files.length > 0
        readonly property bool canVoice: c.editing === null && !!(c.caps && c.caps.message_kinds && c.caps.message_kinds.indexOf("voice") >= 0)
        name: hasContent ? "send" : "mic"
        tip: hasContent ? Conversations.tr("Send") : Conversations.tr("Record a voice message")
        enabledState: !!(hasContent || canVoice)
        onClicked: hasContent ? c.submit() : c.startRecording()
    }
    Rectangle {
        id: field
        anchors { left: attach.right; right: side.left; bottom: parent.bottom; leftMargin: FiliTheme.space1; rightMargin: FiliTheme.space1; bottomMargin: FiliTheme.space2 }
        height: input.height
        radius: FiliTheme.radiusControl
        color: FiliTheme.surface
        border.width: FiliTheme.borderWidth
        border.color: input.activeFocus ? FiliTheme.accentColor : FiliTheme.border
    }
    // While recording: the box shows the time, and cancel / send
    Rectangle {
        id: recBar
        visible: c.recording
        z: 4
        anchors { left: attach.right; right: parent.right; bottom: parent.bottom; leftMargin: FiliTheme.space1; rightMargin: FiliTheme.margin - FiliTheme.space1; bottomMargin: FiliTheme.space2 }
        height: 32
        radius: FiliTheme.radiusControl
        color: FiliTheme.surface
        border.width: FiliTheme.borderWidth
        border.color: FiliTheme.danger
        Rectangle {
            id: recDot
            anchors { left: parent.left; leftMargin: FiliTheme.space3; verticalCenter: parent.verticalCenter }
            width: 10; height: 10; radius: 5
            color: FiliTheme.danger
            SequentialAnimation on opacity { running: recBar.visible; loops: Animation.Infinite; NumberAnimation { to: 0.3; duration: 600 } NumberAnimation { to: 1; duration: 600 } }
        }
        Text {
            anchors { left: recDot.right; leftMargin: FiliTheme.space2; verticalCenter: parent.verticalCenter }
            text: c.clock(c.elapsed) + "   " + Conversations.tr("Recording…")
            font.family: FiliTheme.font
            font.pixelSize: FiliTheme.size
            color: FiliTheme.text
        }
        Row {
            anchors { right: parent.right; rightMargin: FiliTheme.space1; verticalCenter: parent.verticalCenter }
            IconButton { name: "x"; tip: Conversations.tr("Cancel"); onClicked: c.stopRecording(false) }
            IconButton { name: "send"; tip: Conversations.tr("Send"); onClicked: c.stopRecording(true) }
        }
        focus: visible
        Keys.onEscapePressed: c.stopRecording(false)
        Keys.onReturnPressed: c.stopRecording(true)
        Keys.onEnterPressed: c.stopRecording(true)
        onVisibleChanged: if (visible) forceActiveFocus(); else input.forceActiveFocus()
    }
    TextArea {
        id: input
        anchors { left: field.left; right: field.right; bottom: field.bottom }
        height: Math.min(140, Math.max(32, contentHeight + 2 * topPadding))
        leftPadding: FiliTheme.space2
        rightPadding: c.choosing ? 52 : FiliTheme.space2
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
