// The conversation view: messages with day separators, the composer under them. Messages are loaded newest page first
// (`messages.list`), older pages on demand, and kept current by the daemon's `message` events.
import QtQuick
import Fili.Im

Rectangle {
    id: view
    color: FiliTheme.background
    property var messages: []
    property bool more: false
    property bool loading: false
    readonly property var chat: win.chatById(win.current)
    readonly property var caps: win.capsOf(chat)
    readonly property bool group: chat ? chat.kind !== "direct" : false

    function load() {
        messages = [];
        more = false;
        if (!win.current) return;
        const id = win.current;
        loading = true;
        win.rpc("messages.list", { chat: id, limit: 60 }, list => {
            loading = false;
            if (id !== win.current) return;
            messages = list;
            more = list.length >= 60;
            list_.positionViewAtEnd();
        }, () => { loading = false; });
    }
    function loadEarlier() {
        if (loading || messages.length === 0) return;
        const id = win.current;
        loading = true;
        const first = messages[0].timestamp;
        win.rpc("messages.list", { chat: id, before: first, limit: 60 }, list => {
            loading = false;
            if (id !== win.current) return;
            const keep = list_.contentHeight - list_.contentY;
            messages = list.concat(messages);
            more = list.length >= 60;
            list_.contentY = list_.contentHeight - keep;
        }, () => { loading = false; });
    }
    function apply(m) {
        const atEnd = list_.atYEnd;
        const i = messages.findIndex(x => x.id === m.id);
        const copy = messages.slice();
        if (i >= 0) copy[i] = m; else copy.push(m);
        messages = copy;
        if (atEnd) list_.positionViewAtEnd();
        if (!m.from_me && i < 0 && win.active) win.rpc("mark_read", { chat: m.chat, receipt: "auto" });
    }
    function byNative(id) { return messages.find(m => m.native_id === id) || null; }
    function focusComposer() { composer.focusInput(); }
    function replyToLast() {
        for (let i = messages.length - 1; i >= 0; i--) if (!messages[i].from_me && !messages[i].deleted) { win.replyTo = messages[i]; break; }
        composer.focusInput();
    }
    function copySelection() { if (messages.length) Conversations.copyText(messages[messages.length - 1].text.plain); }
    function selectAll() { }
    function dayLabel(ms) {
        const d = new Date(ms), now = new Date();
        if (d.toDateString() === now.toDateString()) return Conversations.tr("Today");
        if (d.toDateString() === new Date(now.getTime() - 86400000).toDateString()) return Conversations.tr("Yesterday");
        return Qt.formatDate(d, "dddd d MMMM" + (d.getFullYear() === now.getFullYear() ? "" : " yyyy"));
    }

    Text {
        anchors.centerIn: parent
        visible: !win.current
        text: Conversations.tr("Choose a conversation")
        font.family: FiliTheme.font
        font.pixelSize: FiliTheme.size
        color: FiliTheme.muted
    }

    ListView {
        id: list_
        visible: win.current !== 0
        anchors { top: parent.top; left: parent.left; right: parent.right; bottom: composer.top }
        clip: true
        model: view.messages
        boundsBehavior: Flickable.StopAtBounds
        topMargin: FiliTheme.space3
        bottomMargin: FiliTheme.space3
        spacing: 2
        KineticScroll {}
        header: Item {
            width: list_.width
            height: view.more ? 32 : 0
            visible: view.more
            TextButton { anchors.centerIn: parent; label: Conversations.tr("Load earlier messages"); onClicked: view.loadEarlier() }
        }
        delegate: Item {
            id: item
            required property var modelData
            required property int index
            readonly property var previous: index > 0 ? view.messages[index - 1] : null
            readonly property bool newDay: !previous || new Date(previous.timestamp).toDateString() !== new Date(modelData.timestamp).toDateString()
            readonly property bool sameSender: previous && !newDay && previous.from_me === modelData.from_me && previous.sender === modelData.sender
            width: list_.width
            height: bubble.height + (newDay ? 34 : 0) + (sameSender ? 0 : 6)
            Text {
                visible: item.newDay
                anchors { horizontalCenter: parent.horizontalCenter; top: parent.top; topMargin: 8 }
                text: view.dayLabel(item.modelData.timestamp)
                font.family: FiliTheme.font
                font.pixelSize: FiliTheme.sizeSmall
                font.weight: FiliTheme.weightAccent
                color: FiliTheme.muted
            }
            Bubble {
                id: bubble
                y: (item.newDay ? 34 : 0) + (item.sameSender ? 0 : 6)
                message: item.modelData
                showSender: view.group && !item.modelData.from_me && !item.sameSender
                quoted: item.modelData.reply_to ? view.byNative(item.modelData.reply_to) : null
                maxWidth: Math.min(560, list_.width * 0.78)
                x: item.modelData.from_me ? list_.width - width - FiliTheme.margin : FiliTheme.margin
                onMenu: (px, py) => {
                    const m = item.modelData;
                    const entries = [];
                    if (view.caps.replies && !m.deleted) entries.push({ id: "reply", label: Conversations.tr("Reply") });
                    if (m.text.plain !== "") entries.push({ id: "copy", label: Conversations.tr("Copy") });
                    if (m.from_me && view.caps.edit && !m.deleted && m.kind === "text" && m.native_id) entries.push({ id: "edit", label: Conversations.tr("Edit…") });
                    if (m.from_me && view.caps.delete && !m.deleted && m.native_id) entries.push({ id: "delete-all", label: Conversations.tr("Delete for Everyone"), danger: true });
                    if (view.caps.reactions && view.caps.reactions !== "none" && m.native_id && !m.deleted)
                        ["👍", "❤️", "😂", "😮", "😢", "🙏"].forEach(e => entries.push({ id: "react:" + e, label: Conversations.tr("React") + " " + e }));
                    if (m.state === "failed") entries.push({ id: "retry", label: Conversations.tr("Send Again") });
                    if (entries.length === 0) return;
                    menu.message = m;
                    const p = bubble.mapToItem(view, px, py);
                    menu.openAt(p.x, p.y, entries);
                }
                onRetry: win.rpc("message.retry", { message: item.modelData.id })
            }
        }
    }
    FiliScrollBar { flick: list_; anchors { top: list_.top; bottom: list_.bottom; right: list_.right } }
    Rectangle {
        visible: view.loading && view.messages.length === 0 && win.current !== 0
        anchors { top: parent.top; left: parent.left; right: parent.right }
        height: 2
        color: FiliTheme.accentColor
    }

    Composer {
        id: composer
        visible: win.current !== 0
        anchors { left: parent.left; right: parent.right; bottom: parent.bottom }
        chat: view.chat
        caps: view.caps
        lastMine: { for (let i = view.messages.length - 1; i >= 0; i--) if (view.messages[i].from_me && !view.messages[i].deleted && view.messages[i].kind === "text") return view.messages[i]; return null; }
    }

    PopupMenu {
        id: menu
        property var message: null
        onActivated: id => {
            const m = message;
            if (id === "reply") { win.replyTo = m; composer.focusInput(); }
            else if (id === "copy") Conversations.copyText(m.text.plain);
            else if (id === "edit") composer.startEdit(m);
            else if (id === "delete-all") win.rpc("delete", { message: m.id, for_everyone: true });
            else if (id.startsWith("react:")) win.rpc("react", { message: m.id, emoji: id.slice(6) });
            else if (id === "retry") win.rpc("message.retry", { message: m.id });
        }
    }
}
