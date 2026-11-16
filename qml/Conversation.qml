// The conversation view: messages with day separators, the composer under them. Messages are loaded newest page first
// (`messages.list`), older pages on demand, and kept current by the daemon's `message` events.
import QtQuick
import Fili.Im

Rectangle {
    id: view
    color: FiliTheme.background
    property var messages: []
    // The list shows a ListModel kept in step with `messages`: a message that comes in or changes is inserted or set in
    // place, so the delegates already built stay as they are (replacing a whole array rebuilds them all and the view jumps)
    property bool incremental: false
    ListModel { id: msgModel; dynamicRoles: true }
    function rebuildModel() {
        msgModel.clear();
        messages.forEach(m => msgModel.append({ msg: m }));
    }
    onMessagesChanged: if (!incremental) rebuildModel()
    property bool more: false
    property bool loading: false
    // Messages received while the view was scrolled up, counted on the "latest messages" button
    property int unseen: 0
    // `person`: who the view is about (the first conversation of the person); `chat`: where a message goes
    readonly property var person: win.chatById(win.current)
    readonly property var chat: win.targetChat
    readonly property var caps: win.capsOf(chat)
    readonly property bool group: person ? person.kind !== "direct" : false
    // ---- Find in the conversation (or in all the conversations of the person) -----------------------------------------
    property bool finding: false
    property string findQuery: ""
    property var hits: []          // matching messages, newest first
    property int hitIndex: 0
    property int highlightId: 0
    function openFind() { finding = true; findField.forceActiveFocus(); findField.selectAll(); }
    function closeFind() { finding = false; hits = []; highlightId = 0; composer.focusInput(); }
    function runFind() {
        const q = findQuery.trim();
        if (q === "") { hits = []; hitIndex = 0; return; }
        const ids = win.scopeIds(), asked = q;
        win.rpc("search", { query: q, chats: ids, limit: 300 }, list => {
            if (asked !== findQuery.trim()) return;
            hits = list;
            hitIndex = 0;
            if (list.length) jumpTo(0);
        });
    }
    function stepFind(step) {
        if (!hits.length) return;
        hitIndex = (hitIndex + step + hits.length) % hits.length;
        jumpTo(hitIndex);
    }
    // Show hit i: older pages are loaded until it is there, then the view centres on it
    function jumpTo(i) {
        const hit = hits[i];
        if (hit) revealMessage(hit.id);
    }
    // Go to a message (a search hit, the original of a quote): older pages are loaded until it is there, then the view
    // centres on it and it flashes
    function revealMessage(id) {
        const show = () => {
            const at = messages.findIndex(m => m.id === id);
            if (at < 0) return;
            list_.follow = false;
            list_.settling = false;
            list_.positionViewAtIndex(at, ListView.Center);
            Qt.callLater(() => list_.positionViewAtIndex(at, ListView.Center));   // once the delegates around it have their real heights
            highlightId = id;
            flash.restart();
        };
        const reveal = () => {
            if (messages.findIndex(m => m.id === id) >= 0) show();
            else if (more && !loading) loadEarlier(reveal);
            else if (loading) Qt.callLater(reveal);
        };
        reveal();
    }
    Timer { id: settle; interval: 5000; onTriggered: { list_.settling = false; if (list_.follow) list_.toEnd(); } }
    Timer { id: flash; interval: 2200; onTriggered: view.highlightId = 0 }
    Timer { id: findDelay; interval: 250; onTriggered: view.runFind() }
    function capsFor(m) { return win.capsOf(win.chatById(m.chat)); }
    function sortMessages(list) { return list.sort((a, b) => (a.timestamp - b.timestamp) || (a.id - b.id)); }

    function toLatest() {
        list_.follow = true;
        // The model is replaced on every change: the delegates are rebuilt with estimated heights, and the position
        // would be lost on the way. Stay at the end until the user scrolls (or a few seconds pass).
        list_.settling = true;
        settle.restart();
        list_.toEnd();
        // The delegates under the end have estimated heights until they are built: settle once they are
        Qt.callLater(() => list_.toEnd());
        unseen = 0;
    }
    function load() {
        messages = [];
        unseen = 0;
        more = false;
        if (!win.current) return;
        const id = win.current, scope = win.scope, ids = win.scopeIds();
        loading = true;
        let waiting = ids.length, acc = [], anyMore = false;
        const done = () => {
            if (--waiting > 0) return;
            loading = false;
            if (id !== win.current || scope !== win.scope) return;
            messages = sortMessages(acc);
            more = anyMore;
            toLatest();
        };
        // Every conversation of the person is read and mixed by date
        ids.forEach(cid => win.rpc("messages.list", { chat: cid, limit: 60 }, list => {
            acc = acc.concat(list);
            if (list.length >= 60) anyMore = true;
            done();
        }, done));
    }
    function loadEarlier(then) {
        if (loading || messages.length === 0) return;
        const id = win.current, scope = win.scope, ids = win.scopeIds();
        loading = true;
        const first = messages[0].timestamp;
        let waiting = ids.length, acc = [], anyMore = false;
        const done = () => {
            if (--waiting > 0) return;
            loading = false;
            if (id !== win.current || scope !== win.scope) return;
            const keep = list_.contentHeight - list_.contentY;
            messages = sortMessages(acc.concat(messages));
            more = anyMore;
            list_.contentY = list_.contentHeight - keep;
            if (then) then();
        };
        ids.forEach(cid => win.rpc("messages.list", { chat: cid, before: first, limit: 60 }, list => {
            acc = acc.concat(list);
            if (list.length >= 60) anyMore = true;
            done();
        }, done));
    }
    function apply(m) {
        // At the bottom, or close to it (a preview or picture may still be growing under the last message), or the
        // user's own new message: the view goes to the very end
        const nearEnd = list_.atYEnd || list_.contentHeight - (list_.contentY + list_.height - list_.bottomMargin) < 160;
        const atEnd = nearEnd || (m.from_me && !messages.some(x => x.id === m.id));
        const i = messages.findIndex(x => x.id === m.id);
        const copy = messages.slice();
        let at = i;
        if (i >= 0) copy[i] = m;
        else { copy.push(m); sortMessages(copy); at = copy.indexOf(m); }
        incremental = true;
        messages = copy;
        incremental = false;
        if (i >= 0) msgModel.set(at, { msg: m }); else msgModel.insert(at, { msg: m });
        if (atEnd) toLatest();
        else if (!m.from_me && i < 0) unseen++;
        if (!m.from_me && i < 0 && win.active) win.rpc("mark_read", { chat: m.chat, receipt: "auto" });
    }
    function byNative(id) { return messages.find(m => m.native_id === id) || null; }
    function focusComposer() { composer.focusInput(); }
    // The message under the pointer: Space opens it in Quick Look when it has a picture, a video or a video link
    property int hoverId: 0
    readonly property var hoverMessage: hoverId ? (messages.find(m => m.id === hoverId) || null) : null
    readonly property bool hoverQuickLookable: hoverMessage !== null && !hoverMessage.deleted && !!(hoverMessage.media || quickLookCard.linkItem(hoverMessage))
    function quickLookHovered() { if (hoverQuickLookable) win.quickLook(hoverMessage.id); }
    // Outside the text field (typing a space must stay possible), and in the field while it is still empty
    Shortcut { sequence: "Space"; enabled: view.hoverQuickLookable && !composer.typing && !win.overlayOpen; onActivated: view.quickLookHovered() }
    // Quick Look on the latest picture, video or video link of the conversation
    function quickLookLast() {
        for (let i = messages.length - 1; i >= 0; i--)
            if (!messages[i].deleted && (messages[i].media || quickLookCard.linkItem(messages[i]))) { win.quickLook(messages[i].id); return; }
    }
    function toggleRecording() { composer.toggleRecording(); }
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
        visible: win.current !== 0 && view.person !== null
        // One row: with several platforms (no account chosen) the tabs fill its whole height on the left, and who it is
        // (picture, name, platform) is on the right; otherwise who it is takes the left
        readonly property bool tabs: win.grouped && !win.accountFocus
        anchors { top: parent.top; left: parent.left; right: parent.right }
        height: visible ? 48 : 0
        color: FiliTheme.background
        Rectangle { anchors { left: parent.left; right: parent.right; bottom: parent.bottom } height: FiliTheme.borderWidth; color: FiliTheme.border }

        // The platforms, as tabs the height of the picture and its margins: All conversations first
        Row {
            id: platformTabs
            visible: head.tabs
            anchors { left: parent.left; top: parent.top; bottom: parent.bottom }
            Repeater {
                model: head.tabs ? [0].concat(win.group.map(c => c.id)) : []
                Rectangle {
                    required property int modelData
                    readonly property var chatOf: modelData === 0 ? null : win.chatById(modelData)
                    readonly property bool on: win.scope === modelData
                    height: platformTabs.height
                    width: tabRow.width + 2 * FiliTheme.space4
                    color: on ? FiliTheme.surface : tabArea.containsMouse ? FiliTheme.alpha(FiliTheme.surfaceHover, 0.6) : "transparent"
                    // The selected tab sits on the bottom line with an accent bar
                    Rectangle { visible: parent.on; anchors { left: parent.left; right: parent.right; bottom: parent.bottom } height: 2; color: FiliTheme.accentColor }
                    Row {
                        id: tabRow
                        anchors.centerIn: parent
                        spacing: FiliTheme.space2
                        Rectangle {
                            visible: parent.parent.chatOf !== null
                            width: 14; height: 14; radius: 7
                            anchors.verticalCenter: parent.verticalCenter
                            color: parent.parent.chatOf ? win.platformColor(win.platformOf(parent.parent.chatOf)) : "transparent"
                            Text { anchors.centerIn: parent; text: parent.parent.parent.chatOf ? win.platformName(win.platformOf(parent.parent.parent.chatOf)).charAt(0) : ""; font.pixelSize: 9; font.bold: true; color: "white" }
                        }
                        Text {
                            anchors.verticalCenter: parent.verticalCenter
                            text: parent.parent.chatOf ? win.platformName(win.platformOf(parent.parent.chatOf)) : Conversations.tr("All conversations")
                            font.family: FiliTheme.font
                            font.pixelSize: FiliTheme.sizeList
                            font.weight: parent.parent.on ? FiliTheme.weightAccent : Font.Normal
                            color: parent.parent.on ? FiliTheme.text : FiliTheme.textSecondary
                        }
                    }
                    MouseArea { id: tabArea; anchors.fill: parent; hoverEnabled: true; cursorShape: Qt.PointingHandCursor; onClicked: win.setScope(modelData) }
                }
            }
        }

        // Who it is: a click opens the contact card
        Item {
            id: identity
            anchors { left: head.tabs ? platformTabs.right : parent.left; right: parent.right; top: parent.top; bottom: parent.bottom }
            MouseArea { anchors.fill: parent; cursorShape: Qt.PointingHandCursor; onClicked: if (view.person) win.showContact(view.person) }
            TextMetrics { id: titleMetrics; text: view.person ? view.person.title : ""; font.family: FiliTheme.font; font.pixelSize: FiliTheme.size + 1; font.weight: FiliTheme.weightAccent }
            Row {
                id: who
                spacing: FiliTheme.space3
                anchors.verticalCenter: parent.verticalCenter
                // On the right when there are tabs, else on the left
                x: head.tabs ? Math.max(FiliTheme.margin, parent.width - width - FiliTheme.margin) : FiliTheme.margin
                layoutDirection: head.tabs ? Qt.RightToLeft : Qt.LeftToRight
                Avatar {
                    id: headAvatar
                    size: 34
                    chatId: view.person ? view.person.id : 0
                    title: view.person ? view.person.title : ""
                    alts: win.groupIds(win.current).filter(x => x !== win.current)
                }
                Column {
                    anchors.verticalCenter: parent.verticalCenter
                    width: Math.max(60, Math.min(titleMetrics.width + 2, identity.width - 34 - 3 * FiliTheme.margin))
                    Text {
                        id: titleText
                        width: parent.width
                        horizontalAlignment: head.tabs ? Text.AlignRight : Text.AlignLeft
                        text: view.person ? view.person.title : ""
                        elide: Text.ElideRight
                        textFormat: Text.PlainText
                        font.family: FiliTheme.font
                        font.pixelSize: FiliTheme.size + 1
                        font.weight: FiliTheme.weightAccent
                        color: FiliTheme.text
                    }
                    Text {
                        visible: win.multi && !head.tabs
                        width: parent.width
                        text: view.chat ? win.platformName(win.platformOf(view.chat)) : ""
                        elide: Text.ElideRight
                        font.family: FiliTheme.font
                        font.pixelSize: FiliTheme.sizeSmall
                        color: FiliTheme.muted
                    }
                }
            }
        }
    }

    Rectangle {
        id: findBar
        visible: view.finding && win.current !== 0
        anchors { top: head.bottom; left: parent.left; right: parent.right }
        height: visible ? 40 : 0
        color: FiliTheme.backgroundDeep
        z: 6
        Rectangle { anchors { left: parent.left; right: parent.right; bottom: parent.bottom } height: FiliTheme.borderWidth; color: FiliTheme.border }
        SearchField {
            id: findField
            anchors { left: parent.left; leftMargin: FiliTheme.margin; right: count.left; rightMargin: FiliTheme.space3; verticalCenter: parent.verticalCenter }
            placeholderText: win.grouped && win.scope === 0 ? Conversations.tr("Find in all the conversations of this contact") : Conversations.tr("Find in this conversation")
            onTextChanged: { view.findQuery = text; findDelay.restart(); }
            Keys.onEscapePressed: view.closeFind()
            Keys.onReturnPressed: e => view.stepFind((e.modifiers & Qt.ShiftModifier) ? 1 : -1)
            Keys.onEnterPressed: e => view.stepFind((e.modifiers & Qt.ShiftModifier) ? 1 : -1)
        }
        Text {
            id: count
            anchors { right: nav.left; rightMargin: FiliTheme.space2; verticalCenter: parent.verticalCenter }
            text: view.findQuery.trim() === "" ? "" : (view.hits.length ? (view.hitIndex + 1) + " / " + view.hits.length : Conversations.tr("No message found"))
            font.family: FiliTheme.font
            font.pixelSize: FiliTheme.sizeList
            color: FiliTheme.textSecondary
        }
        Row {
            id: nav
            anchors { right: parent.right; rightMargin: FiliTheme.margin; verticalCenter: parent.verticalCenter }
            IconButton { name: "chevron-up"; tip: Conversations.tr("Older match"); enabledState: view.hits.length > 0; onClicked: view.stepFind(1) }
            IconButton { name: "chevron-down"; tip: Conversations.tr("Newer match"); enabledState: view.hits.length > 0; onClicked: view.stepFind(-1) }
            IconButton { name: "x"; tip: Conversations.tr("Close"); onClicked: view.closeFind() }
        }
    }

    ListView {
        id: list_
        visible: win.current !== 0
        anchors { top: findBar.visible ? findBar.bottom : head.bottom; left: parent.left; right: parent.right; bottom: composer.top }
        clip: true
        model: msgModel
        boundsBehavior: Flickable.StopAtBounds
        topMargin: FiliTheme.space3
        bottomMargin: FiliTheme.space3
        spacing: 2
        KineticScroll { id: kinetic }
        // The first touch of the user ends the "stay at the end" of a freshly opened conversation
        Connections { target: kinetic; function onWheel() { list_.settling = false; } }
        onMovementStarted: settling = false
        onAtYEndChanged: if (atYEnd) view.unseen = 0
        // The view stays at the bottom while the composer grows (long text, reply bar, files) or messages come in,
        // as long as it was at the bottom: `follow` changes only when the content moves (the user scrolls)
        // positionViewAtEnd() stops short of the bottom margin (the last bubble then touches the composer and the view
        // is not "at the end"): go to the real end, margin included
        function toEnd() {
            positionViewAtEnd();
            contentY = Math.max(originY, originY + contentHeight - height + bottomMargin);
        }
        property bool follow: true
        property bool settling: false   // just after a conversation opens: the end is kept whatever the estimated heights do
        onContentYChanged: if (!settling) follow = atYEnd
        onHeightChanged: if (follow) toEnd()
        onContentHeightChanged: if (follow || settling) Qt.callLater(toEnd)
        header: Item {
            width: list_.width
            height: view.more ? 32 : 0
            visible: view.more
            TextButton { anchors.centerIn: parent; label: Conversations.tr("Load earlier messages"); onClicked: view.loadEarlier() }
        }
        delegate: Item {
            id: item
            // The hovered message (and its action bar) is drawn above the others, whatever the order they were built in
            z: hover.hovered || toolsHover.hovered ? 100 : 1
            required property var msg
            readonly property var modelData: msg
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
                highlighted: item.modelData.id === view.highlightId
                showPlatform: win.scope === 0 && win.grouped
                showSender: view.group && !item.modelData.from_me && !item.sameSender
                quoted: item.modelData.reply_to ? view.byNative(item.modelData.reply_to) : null
                maxWidth: Math.min(560, list_.width * 0.78 - (win.scope === 0 && win.grouped ? 26 : 0))
                x: item.modelData.from_me ? list_.width - width - FiliTheme.margin : FiliTheme.margin + (win.scope === 0 && win.grouped ? 26 : 0)
                onMenu: (px, py) => {
                    const p = bubble.mapToItem(view, px, py);
                    view.openMenu(item.modelData, p.x, p.y, "all");
                }
                onRetry: win.rpc("message.retry", { message: item.modelData.id })
                onQuoteClicked: if (bubble.quoted) view.revealMessage(bubble.quoted.id)
            }
            // The platform of the message, on its left (the views that mix several)
            Rectangle {
                visible: win.scope === 0 && win.grouped
                x: bubble.x - 24
                y: bubble.y + 4
                width: 18; height: 18; radius: 9
                color: win.platformColor(win.platformOf(win.chatById(item.modelData.chat)))
                Text { anchors.centerIn: parent; text: win.platformName(win.platformOf(win.chatById(item.modelData.chat))).charAt(0); font.pixelSize: 11; font.bold: true; color: "white" }
            }
            // Actions shown while the pointer is over the message, on its bubble
            HoverHandler { id: hover; onHoveredChanged: { if (hovered) view.hoverId = item.modelData.id; else if (view.hoverId === item.modelData.id) view.hoverId = 0; } }
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
                        visible: view.capsFor(tools.m).replies
                        name: "reply"
                        tip: Conversations.tr("Reply")
                        onClicked: { win.targetPick = tools.m.chat; win.replyTo = tools.m; composer.focusInput(); }
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
        onPlatformMenu: (x, y) => { const p = composer.mapToItem(view, x, y); menu.message = null; menu.openAt(p.x, p.y - 10 - win.group.length * 26, win.group.map(c => ({ id: "target:" + c.id, label: (c.id === win.targetChat.id ? "● " : "○ ") + win.platformName(win.platformOf(c)) }))); }
        onSent: view.toLatest()
        quickLookReady: view.hoverQuickLookable
        onQuickLook: view.quickLookHovered()
        lastMine: { for (let i = view.messages.length - 1; i >= 0; i--) if (view.messages[i].from_me && !view.messages[i].deleted && view.messages[i].kind === "text") return view.messages[i]; return null; }
    }

    function canReact(m) { const c = capsFor(m); return c.reactions && c.reactions !== "none" && m.native_id && !m.deleted; }
    // "react": the quick emojis and "More…"; "all": what the hover bar does not already offer
    readonly property var quickEmojis: ["👍", "❤️", "😂", "😮", "😢", "🙏"]
    function openMenu(m, x, y, mode) {
        const entries = [];
        if (mode === "react") {
            quickEmojis.forEach(e => entries.push({ id: "react:" + e, label: e }));
            entries.push({ id: "react-more", label: Conversations.tr("More emoji…") });
        } else {
            if (m.text.plain !== "") entries.push({ id: "copy", label: Conversations.tr("Copy") });
            if (m.from_me && capsFor(m).edit && !m.deleted && m.kind === "text" && m.native_id) entries.push({ id: "edit", label: Conversations.tr("Edit…") });
            entries.push({ id: "delete-local", label: Conversations.tr("Delete for Me"), danger: true });
            if (m.from_me && capsFor(m).delete && !m.deleted && m.native_id) entries.push({ id: "delete-all", label: Conversations.tr("Delete for Everyone"), danger: true });
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
            if (id.startsWith("target:")) { win.targetPick = parseInt(id.slice(7)); composer.focusInput(); return; }
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
