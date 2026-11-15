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
    // Messages received while the view was scrolled up, counted on the "latest messages" button
    property int unseen: 0
    readonly property var chat: win.chatById(win.current)
    readonly property var caps: win.capsOf(chat)
    readonly property bool group: chat ? chat.kind !== "direct" : false

    function toLatest() {
        list_.positionViewAtEnd();
        // The delegates under the end have estimated heights until they are built: settle once they are
        Qt.callLater(() => list_.positionViewAtEnd());
        unseen = 0;
    }
    function load() {
        messages = [];
        unseen = 0;
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
        else if (!m.from_me && i < 0) unseen++;
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

    // Who it is: picture and name, a click opens the contact card
    Rectangle {
        id: head
        visible: win.current !== 0 && view.chat !== null
        anchors { top: parent.top; left: parent.left; right: parent.right }
        height: visible ? 48 : 0
        color: FiliTheme.background
        Rectangle { anchors { left: parent.left; right: parent.right; bottom: parent.bottom } height: FiliTheme.borderWidth; color: FiliTheme.border }
        Avatar {
            id: headAvatar
            anchors { left: parent.left; leftMargin: FiliTheme.margin; verticalCenter: parent.verticalCenter }
            size: 34
            chatId: view.chat ? view.chat.id : 0
            title: view.chat ? view.chat.title : ""
            platform: win.multi ? win.platformOf(view.chat) : ""
        }
        Column {
            anchors { left: headAvatar.right; leftMargin: FiliTheme.space3; right: parent.right; rightMargin: FiliTheme.margin; verticalCenter: parent.verticalCenter }
            Text {
                width: parent.width
                text: view.chat ? view.chat.title : ""
                elide: Text.ElideRight
                textFormat: Text.PlainText
                font.family: FiliTheme.font
                font.pixelSize: FiliTheme.size + 1
                font.weight: FiliTheme.weightAccent
                color: FiliTheme.text
            }
            Text {
                visible: win.multi
                width: parent.width
                text: view.chat ? win.platformName(win.platformOf(view.chat)) : ""
                elide: Text.ElideRight
                font.family: FiliTheme.font
                font.pixelSize: FiliTheme.sizeSmall
                color: FiliTheme.muted
            }
        }
        MouseArea { anchors.fill: parent; cursorShape: Qt.PointingHandCursor; onClicked: if (view.chat) win.showContact(view.chat) }
    }

    ListView {
        id: list_
        visible: win.current !== 0
        anchors { top: head.bottom; left: parent.left; right: parent.right; bottom: composer.top }
        clip: true
        model: view.messages
        boundsBehavior: Flickable.StopAtBounds
        topMargin: FiliTheme.space3
        bottomMargin: FiliTheme.space3
        spacing: 2
        KineticScroll {}
        onAtYEndChanged: if (atYEnd) view.unseen = 0
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
                    const p = bubble.mapToItem(view, px, py);
                    view.openMenu(item.modelData, p.x, p.y, "all");
                }
                onRetry: win.rpc("message.retry", { message: item.modelData.id })
            }
            // Actions shown while the pointer is over the message, on its bubble
            HoverHandler { id: hover }
            Rectangle {
                id: tools
                readonly property var m: item.modelData
                visible: (hover.hovered || toolsHover.hovered) && !m.deleted
                opacity: visible ? 1 : 0
                height: 26
                width: toolsRow.width + 4
                radius: FiliTheme.radiusControl
                color: FiliTheme.surface
                border.width: FiliTheme.borderWidth
                border.color: FiliTheme.border
                // Astride the top edge of the bubble, from its outer corner inwards: in the window whatever the width, and the text stays readable
                y: bubble.y - 20
                x: m.from_me ? bubble.x + bubble.width - width - 4 : bubble.x + 4
                z: 5
                HoverHandler { id: toolsHover }
                Row {
                    id: toolsRow
                    x: 2
                    y: 1
                    IconButton {
                        visible: view.caps.replies
                        name: "reply"
                        tip: Conversations.tr("Reply")
                        onClicked: { win.replyTo = tools.m; composer.focusInput(); }
                    }
                    IconButton {
                        name: "forward"
                        tip: Conversations.tr("Forward")
                        onClicked: win.forwardMessage(tools.m)
                    }
                    IconButton {
                        visible: view.canReact(tools.m)
                        name: "smile"
                        tip: Conversations.tr("React")
                        onClicked: { const p = tools.mapToItem(view, 0, tools.height); view.openMenu(tools.m, p.x, p.y, "react"); }
                    }
                    IconButton {
                        visible: tools.m.media !== null && tools.m.media !== undefined
                        name: "download"
                        tip: Conversations.tr("Open")
                        onClicked: win.quickLook(tools.m.id)
                    }
                    IconButton {
                        name: "more"
                        tip: Conversations.tr("More")
                        onClicked: { const p = tools.mapToItem(view, 0, tools.height); view.openMenu(tools.m, p.x, p.y, "all"); }
                    }
                }
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

    // One click down to the latest message; shown only when the view is not already at the end
    Rectangle {
        id: latest
        readonly property bool shown: win.current !== 0 && !list_.atYEnd && view.messages.length > 0
        visible: opacity > 0
        opacity: shown ? 1 : 0
        Behavior on opacity { NumberAnimation { duration: 120 } }
        z: 5
        anchors { right: parent.right; rightMargin: FiliTheme.margin; bottom: composer.top; bottomMargin: FiliTheme.space2 }
        width: badge.visible ? 28 + badge.width + FiliTheme.space2 : 32
        height: 32
        radius: FiliTheme.radiusControl
        color: FiliTheme.surface
        border.width: FiliTheme.borderWidth
        border.color: FiliTheme.border
        IconButton {
            anchors { left: parent.left; leftMargin: 2; verticalCenter: parent.verticalCenter }
            width: 28; height: 28
            name: "chevron-down"
            tip: Conversations.tr("Latest messages")
            onClicked: view.toLatest()
        }
        Text {
            id: badge
            visible: view.unseen > 0
            anchors { right: parent.right; rightMargin: FiliTheme.space2; verticalCenter: parent.verticalCenter }
            text: view.unseen > 99 ? "99+" : view.unseen
            font.family: FiliTheme.font
            font.pixelSize: FiliTheme.sizeSmall
            font.weight: FiliTheme.weightAccent
            color: FiliTheme.accentColor
        }
    }

    Composer {
        id: composer
        visible: win.current !== 0
        anchors { left: parent.left; right: parent.right; bottom: parent.bottom }
        chat: view.chat
        caps: view.caps
        lastMine: { for (let i = view.messages.length - 1; i >= 0; i--) if (view.messages[i].from_me && !view.messages[i].deleted && view.messages[i].kind === "text") return view.messages[i]; return null; }
    }

    function canReact(m) { return caps.reactions && caps.reactions !== "none" && m.native_id && !m.deleted; }
    // "react": the quick emojis and "More…"; "all": what the hover bar does not already offer
    readonly property var quickEmojis: ["👍", "❤️", "😂", "😮", "😢", "🙏"]
    function openMenu(m, x, y, mode) {
        const entries = [];
        if (mode === "react") {
            quickEmojis.forEach(e => entries.push({ id: "react:" + e, label: e }));
            entries.push({ id: "react-more", label: Conversations.tr("More emoji…") });
        } else {
            if (m.text.plain !== "") entries.push({ id: "copy", label: Conversations.tr("Copy") });
            if (m.from_me && caps.edit && !m.deleted && m.kind === "text" && m.native_id) entries.push({ id: "edit", label: Conversations.tr("Edit…") });
            entries.push({ id: "delete-local", label: Conversations.tr("Delete for Me"), danger: true });
            if (m.from_me && caps.delete && !m.deleted && m.native_id) entries.push({ id: "delete-all", label: Conversations.tr("Delete for Everyone"), danger: true });
            if (m.state === "failed") entries.push({ id: "retry", label: Conversations.tr("Send Again") });
        }
        if (entries.length === 0) return;
        menu.message = m;
        menu.openAt(x, y, entries);
    }

    PopupMenu {
        id: menu
        property var message: null
        onActivated: id => {
            const m = message;
            if (id === "react-more") win.pickEmoji(m);
            else if (id === "copy") Conversations.copyText(m.text.plain);
            else if (id === "edit") composer.startEdit(m);
            else if (id === "delete-local") win.rpc("delete", { message: m.id, local: true }, () => { view.messages = view.messages.filter(x => x.id !== m.id); });
            else if (id === "delete-all") win.rpc("delete", { message: m.id, for_everyone: true });
            else if (id.startsWith("react:")) win.rpc("react", { message: m.id, emoji: id.slice(6) });
            else if (id === "retry") win.rpc("message.retry", { message: m.id });
        }
    }
}
