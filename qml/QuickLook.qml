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
    property var paths: ({})     // saved copies (pictures, documents)
    property var streams: ({})   // local streaming addresses (video, audio): played without a download
    property var failed: ({})
    property var inflight: ({})
    readonly property var current: items.length > 0 ? items[index] : null
    // An item: {id (message), m (message), kind (image, video, audio, file, ... or "youtube" / "videourl" for a link), media, url}
    readonly property string kind: current ? current.kind : ""
    readonly property string path: current ? (current.kind === "videourl" ? current.url : (paths[current.id] || streams[current.id] || "")) : ""
    readonly property bool remote: path.indexOf("http") === 0
    readonly property bool isYoutube: kind === "youtube"
    readonly property bool isPage: kind === "youtube" || kind === "sitevideo"
    property var titles: ({})
    property string playerError: ""
    readonly property bool isPicture: kind === "image" || kind === "sticker"
    readonly property bool isVideo: kind === "video" || kind === "videourl"
    readonly property bool isAudio: kind === "audio" || kind === "voice"
    readonly property bool playable: isVideo || isAudio

    // The media of `messages`, oldest first; opens on the message `id`
    function linkItem(m) {
        const url = (/https?:\/\/[^\s<>"']+/.exec(m.text.plain) || [""])[0];
        if (url === "") return null;
        const yt = /(?:youtube\.com\/(?:watch\?(?:[^\s#]*&)?v=|shorts\/|embed\/|live\/)|youtu\.be\/)([\w-]{11})/.exec(url);
        if (yt) return Conversations.linkPreviews() ? { id: m.id, m: m, kind: "youtube", url: url, vid: yt[1] } : null;
        if (/\.(mp4|webm|mov|m4v|m3u8)(\?|#|$)/i.test(url)) return { id: m.id, m: m, kind: "videourl", url: url };
        if (/^https?:\/\/(www\.)?(instagram\.com\/(reel|reels|p|tv)\/|tiktok\.com\/|vimeo\.com\/\d|dailymotion\.com\/video|(x|twitter)\.com\/[^\/]+\/status|facebook\.com\/(watch|reel)|fb\.watch|clips\.twitch\.tv|twitch\.tv\/[^\/]+\/clip)/i.test(url)) return { id: m.id, m: m, kind: "sitevideo", url: url };
        return null;
    }
    property int startAt: 0   // where a video page continues from (ms), when opened from a playing bubble
    function open(messages, id, at) {
        startAt = at || 0;
        win.stopInline();
        const list = [];
        for (const m of messages) {
            if (m.deleted) continue;
            if (m.media) list.push({ id: m.id, m: m, kind: m.media.kind, media: m.media });
            else { const l = linkItem(m); if (l) list.push(l); }
        }
        items = list;
        const i = items.findIndex(m => m.id === id);
        if (i < 0) return;
        index = i;
        visible = true;
        forceActiveFocus();
        ensure(index);
    }
    function close() { player.stop(); pageVideo.stop(); visible = false; }
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
            if (!m.media) { loadTitle(m); continue; }
            if (paths[m.id] || streams[m.id] || failed[m.id]) continue;
            if (m.kind === "video" || m.kind === "audio" || m.kind === "voice") { if (k === i) stream(m); continue; }
            if (k !== i && (m.media.size || 0) > 30000000) continue;
            fetch(m);
        }
    }
    function loadTitle(m) {
        if (m.kind !== "youtube" || titles[m.vid] !== undefined) return;
        titles[m.vid] = "";
        const x = new XMLHttpRequest();
        x.onreadystatechange = () => {
            if (x.readyState !== XMLHttpRequest.DONE || x.status !== 200) return;
            try { const t = Object.assign({}, ql.titles); t[m.vid] = JSON.parse(x.responseText).title || ""; ql.titles = t; } catch (e) { }
        };
        x.open("GET", "https://www.youtube.com/oembed?format=json&url=" + encodeURIComponent("https://www.youtube.com/watch?v=" + m.vid));
        x.send();
    }
    function playLink() {
        if (!current || !current.url) return;
        if (!Conversations.playLink(current.url)) Conversations.openUrl(current.url);
    }
    // Video and audio: the connector serves them over local HTTP, the player reads what it needs (or a saved copy)
    function stream(m) {
        if (inflight["s" + m.id]) return;
        inflight["s" + m.id] = true;
        win.rpc("media.stream", { message: m.id }, r => {
            if (r.path) { const p = Object.assign({}, paths); p[m.id] = r.path; paths = p; }
            else { const s = Object.assign({}, streams); s[m.id] = r.url; streams = s; }
        }, e => {
            // Cannot be streamed (no address, expired link): save it, then play the copy
            inflight["s" + m.id] = false;
            fetch(m);
        });
    }
    // A saved copy for the default application
    function openWithApp() {
        if (!current) return;
        if (current.url) { Conversations.openUrl(current.url); return; }
        const m = current;
        if (paths[m.id]) { Conversations.openUrl("file://" + encodeURI(paths[m.id])); return; }
        win.rpc("media.fetch", { message: m.id, retry: true }, r => Conversations.openUrl("file://" + encodeURI(r.path)));
    }
    function fetch(m) {
        if (inflight[m.id]) return;
        inflight[m.id] = true;
        win.rpc("media.fetch", { message: m.id, retry: true }, r => {
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
        autoplay = false;
        if (player.playbackState === MediaPlayer.PlayingState) player.pause(); else player.play();
    }
    // A new file starts playing as soon as the player has loaded it
    property bool autoplay: true
    onPathChanged: {
        player.stop();
        autoplay = true;
        playerError = "";
        player.source = playable && path !== "" ? (remote ? path : "file://" + encodeURI(path)) : "";
    }
    onKindChanged: if (!playable) player.source = ""

    Keys.onEscapePressed: close()
    Keys.onLeftPressed: go(-1)
    Keys.onRightPressed: go(1)
    Keys.onSpacePressed: isPage ? close() : playable && path !== "" ? togglePlay() : close()

    MediaDevices { id: outputs }
    MediaPlayer {
        id: player
        onMediaStatusChanged: if (mediaStatus === MediaPlayer.LoadedMedia && ql.autoplay && ql.visible) play()
        onErrorOccurred: (error, text) => {
            // A stream that cannot be read: save the file and play the copy
            if (ql.remote && ql.current && ql.kind !== "videourl" && !ql.paths[ql.current.id]) ql.fetch(ql.current);
            else ql.playerError = text;
        }
        videoOutput: video
        // Follows the default output of the system: this player lives as long as the window, so a fixed AudioOutput
        // would keep the output found at launch (the internal speakers) after the default changed
        audioOutput: AudioOutput { device: outputs.defaultAudioOutput }
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
            text: ql.current ? ((ql.current.m.sender_name || (ql.current.m.from_me ? Conversations.tr("You") : "")) + (ql.current.media && ql.current.media.name ? " · " + ql.current.media.name : ql.current.url ? " · " + ql.current.url : "")) : ""
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
            IconButton { name: "forward"; tip: Conversations.tr("Forward"); onClicked: { const m = ql.current.m; ql.close(); win.forwardMessage(m); } }
            IconButton { name: "download"; tip: Conversations.tr("Open with the default application"); enabledState: ql.path !== "" || ql.isPage; onClicked: ql.openWithApp() }
            IconButton { name: "x"; tip: Conversations.tr("Close"); onClicked: ql.close() }
        }
    }

    // Content
    Item {
        id: stage
        anchors { left: parent.left; right: parent.right; top: top.bottom; bottom: bar.top; leftMargin: 64; rightMargin: 64 }
        MouseArea { anchors.fill: parent; onClicked: ql.playable && ql.path !== "" ? ql.togglePlay() : undefined }
        readonly property bool animatedFile: /\.(gif|webp|apng)$/i.test(ql.path)
        Image {
            anchors.fill: parent
            visible: ql.isPicture && ql.path !== "" && !stage.animatedFile
            source: visible ? "file://" + encodeURI(ql.path) : ""
            fillMode: Image.PreserveAspectFit
            asynchronous: true
            cache: false
            autoTransform: true
            sourceSize.width: 2400
        }
        AnimatedImage {
            anchors.fill: parent
            playing: visible
            visible: ql.isPicture && ql.path !== "" && stage.animatedFile
            source: visible ? "file://" + encodeURI(ql.path) : ""
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
            visible: (ql.isAudio || (!ql.isPicture && !ql.isVideo && !ql.isPage)) && ql.current !== null
            spacing: FiliTheme.space2
            Text {
                anchors.horizontalCenter: parent.horizontalCenter
                text: ql.isAudio ? "♪" : "▣"
                font.pixelSize: 64
                color: FiliTheme.accentColor
            }
            Text {
                anchors.horizontalCenter: parent.horizontalCenter
                text: ql.current && ql.current.media ? (ql.current.media.name || ql.current.media.kind) : ""
                textFormat: Text.PlainText
                font.family: FiliTheme.font
                font.pixelSize: FiliTheme.size
                color: FiliTheme.text
            }
            Text {
                anchors.horizontalCenter: parent.horizontalCenter
                text: ql.current && ql.current.media ? ql.sizeText(ql.current.media.size) : ""
                font.family: FiliTheme.font
                font.pixelSize: FiliTheme.sizeList
                color: FiliTheme.muted
            }
            TextButton {
                anchors.horizontalCenter: parent.horizontalCenter
                visible: !ql.isAudio && ql.path !== ""
                label: Conversations.tr("Open")
                onClicked: ql.openWithApp()
            }
        }
        // A video page (YouTube, Instagram, TikTok…): played here, full size, once its stream is found
        VideoLink {
            id: pageVideo
            anchors.fill: parent
            visible: ql.isPage
            url: ql.isPage ? ql.current.url : ""
            poster: ql.isYoutube ? "https://i.ytimg.com/vi/" + ql.current.vid + "/hqdefault.jpg" : ""
            autoplay: ql.visible
            startAt: ql.startAt
        }
        // Waiting / failure
        Spinner {
            anchors.centerIn: parent
            size: 140
            onDark: true
            backdrop: true
            visible: ql.current !== null && ql.path === "" && (ql.isPicture || ql.isVideo) && ql.playerError === "" && !ql.failed[ql.current.id]
        }
        Text {
            anchors.centerIn: parent
            visible: ql.current !== null && ((ql.path === "" && (ql.isPicture || ql.isVideo) && !!ql.failed[ql.current.id]) || ql.playerError !== "")
            text: ql.playerError !== "" ? ql.playerError : ql.current && ql.failed[ql.current.id] ? ql.failed[ql.current.id] : ""
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
