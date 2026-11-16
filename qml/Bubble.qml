// One message: sender (groups), quoted message, text (plain, selectable), media chip, reactions, time and state.
import QtQuick
import Fili.Im

Rectangle {
    id: b
    property var message
    property var quoted: null
    property bool showSender: false
    // The platform the message came or went through, when several are mixed
    property bool showPlatform: false
    readonly property string platform: showPlatform ? win.platformOf(win.chatById(message.chat)) : ""
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
        return (kinds[m.kind] ? Conversations.tr(kinds[m.kind]) : m.kind) + (m.name && m.kind !== "voice" ? ": " + m.name : "");
    }
    readonly property string body: message.deleted ? Conversations.tr("Message deleted") : message.text.plain
    // Pictures show in the bubble (downloaded when it appears); videos and files download when clicked, then open
    readonly property bool hasMedia: message.media !== null && message.media !== undefined && !message.deleted
    readonly property bool isPicture: hasMedia && (message.media.kind === "image" || message.media.kind === "sticker")
    readonly property bool isVideo: hasMedia && message.media.kind === "video"
    property string thumbPath: ""
    readonly property bool showsPicture: isPicture || (isVideo && thumbPath !== "")
    // A link in the text: YouTube gets a card (thumbnail and title), a direct video file a chip; both play in Quick Look
    readonly property string linkUrl: { const m = /https?:\/\/[^\s<>"']+/.exec(b.body); return m ? m[0] : ""; }
    readonly property string ytId: { const m = /(?:youtube\.com\/(?:watch\?(?:[^\s#]*&)?v=|shorts\/|embed\/|live\/)|youtu\.be\/)([\w-]{11})/.exec(b.linkUrl); return m ? m[1] : ""; }
    readonly property bool videoLink: ytId === "" && /\.(mp4|webm|mov|m4v|m3u8)(\?|#|$)/i.test(linkUrl)
    readonly property bool previewsOn: Conversations.linkPreviews()
    readonly property bool card: ytId !== "" && previewsOn && !message.deleted
    property string ytTitle: ""
    // Any other link: what the page says about itself (Open Graph), when [links] previews allows it
    readonly property string linkMode: Conversations.linkMode()
    property var og: ({})
    property int ogId: 0
    readonly property bool ogWanted: linkUrl !== "" && ytId === "" && !videoLink && linkMode === "all" && !message.deleted && !hasMedia
    // Pages that hold a video yt-dlp can play (other than YouTube, which has its own card)
    readonly property bool siteVideo: ytId === "" && /^https?:\/\/(www\.)?(instagram\.com\/(reel|reels|p|tv)\/|tiktok\.com\/|vimeo\.com\/\d|dailymotion\.com\/video|(x|twitter)\.com\/[^\/]+\/status|facebook\.com\/(watch|reel)|fb\.watch|clips\.twitch\.tv|twitch\.tv\/[^\/]+\/clip)/i.test(linkUrl)
    readonly property bool ogShown: ogWanted && (og.title !== undefined || og.image !== undefined)
    Connections {
        target: Conversations
        function onLinkPreview(id, json) { if (id === b.ogId) { try { b.og = JSON.parse(json); } catch (e) { } } }
    }
    // What the file is, under it: type, format, size
    function formatOf(m) {
        const mime = (m.mime || "").split(";")[0];
        const ext = (m.name || "").indexOf(".") >= 0 ? m.name.split(".").pop() : "";
        let f = (ext || (mime.indexOf("/") > 0 ? mime.split("/")[1] : "")).toUpperCase();
        return ({ "X-MATROSKA": "MKV", "QUICKTIME": "MOV", "X-M4A": "M4A", "MPEG": "MP3", "VND.OPENXMLFORMATS-OFFICEDOCUMENT.WORDPROCESSINGML.DOCUMENT": "DOCX" })[f] || f;
    }
    readonly property string mediaInfo: {
        if (!hasMedia) return "";
        const m = message.media;
        const fmt = formatOf(m);
        const gif = fmt === "GIF";
        const words = { image: "Image", video: "Video", audio: "Audio", voice: "Voice message", file: "Document", document: "Document", sticker: "Sticker" };
        const kind = gif ? "GIF" : (words[m.kind] ? Conversations.tr(words[m.kind]) : m.kind);
        return [kind, fmt !== kind.toUpperCase() ? fmt : "", sizeText(m.size)].filter(x => x !== "").join(" · ");
    }
    readonly property string linkSource: {
        if (card) return "YouTube";
        if (!videoLink) return "";
        const m = /^https?:\/\/([^\/?#]+)/.exec(linkUrl);
        const ext = (/\.(mp4|webm|mov|m4v|m3u8)(\?|#|$)/i.exec(linkUrl) || [])[1];
        return [Conversations.tr("Video link"), ext ? ext.toUpperCase() : "", m ? m[1] : ""].filter(x => x !== "").join(" · ");
    }
    readonly property real pictureWidth: Math.min(280, b.inner)
    property string mediaPath: ""
    property bool fetching: false
    property bool expired: false      // the server no longer has the file: a click asks the phone for it again
    function fetchMedia(thenOpen) {
        if (fetching) return;
        if (mediaPath !== "") { if (thenOpen) win.quickLook(message.id); return; }
        fetching = true;
        // Only a click asks the phone again (automatic fetches stay quiet)
        win.rpc("media.fetch", { message: message.id, retry: thenOpen }, r => {
            expired = false;
            fetching = false;
            mediaPath = r.path;
            thumbPath = r.thumb || "";
            if (thenOpen) win.quickLook(message.id);
        }, e => {
            fetching = false;
            expired = !!(e && e.code === "expired");
            // Pictures fetched on their own (expired files, offline) stay quiet; a click that fails says so
            if (thenOpen) win.notice = (e && e.message) || Conversations.tr("The file could not be downloaded");
        });
    }
    function sizeText(n) {
        if (!n) return "";
        if (n < 1024) return n + " B";
        if (n < 1048576) return Math.round(n / 1024) + " KB";
        return (n / 1048576).toFixed(1) + " MB";
    }
    Component.onCompleted: {
        if (ogWanted) ogId = Conversations.previewLink(linkUrl);
        if (isPicture && (message.media.size || 0) < 15000000) fetchMedia(false);
        // A video shows a still picture made from its stream: nothing is downloaded for it
        if (isVideo) win.rpc("media.thumb", { message: message.id }, r => { thumbPath = r.path; }, () => { });
        if (card) {
            const x = new XMLHttpRequest();
            x.onreadystatechange = () => {
                if (x.readyState !== XMLHttpRequest.DONE || x.status !== 200) return;
                try { b.ytTitle = JSON.parse(x.responseText).title || ""; } catch (e) { }
            };
            x.open("GET", "https://www.youtube.com/oembed?format=json&url=" + encodeURIComponent("https://www.youtube.com/watch?v=" + ytId));
            x.send();
        }
    }

    // The bubble is as wide as its widest line (text measured unwrapped), up to maxWidth; everything inside follows it
    readonly property real innerWidth: (showsPicture || card || ogShown) ? pictureWidth : b.body.indexOf("\n") >= 0 ? b.inner : Math.min(b.inner, Math.max(probe.width, probeSender.width, b.hasMedia && !b.showsPicture ? 200 : 0, foot.implicitWidth, 60) + 2)
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
    property bool highlighted: false
    border.width: failed || highlighted ? FiliTheme.borderWidth : 0
    border.color: failed ? FiliTheme.danger : FiliTheme.accentColor

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
            visible: b.showsPicture
            width: parent.width
            // Stills (and the still of a video) use Image, which honours the orientation of photos; only GIF, WebP and APNG animate
            readonly property bool animatedFile: b.isPicture && /\.(gif|webp|apng)$/i.test(b.mediaPath)
            readonly property var shown: animatedFile ? anim : image
            height: visible ? (shown.status === Image.Ready && shown.implicitWidth > 0 ? Math.min(360, width * shown.implicitHeight / shown.implicitWidth) : 120) : 0
            radius: FiliTheme.radiusControl
            color: FiliTheme.backgroundDeep
            clip: true
            Image {
                id: image
                anchors.fill: parent
                visible: !picture.animatedFile
                source: picture.animatedFile ? "" : b.isVideo ? (b.thumbPath !== "" ? "file://" + encodeURI(b.thumbPath) : "") : b.mediaPath !== "" ? "file://" + encodeURI(b.mediaPath) : ""
                asynchronous: true
                cache: true
                autoTransform: true
                sourceSize.width: 640
                fillMode: Image.PreserveAspectFit
            }
            AnimatedImage {
                id: anim
                anchors.fill: parent
                visible: picture.animatedFile
                source: picture.animatedFile ? "file://" + encodeURI(b.mediaPath) : ""
                asynchronous: true
                cache: true
                playing: visible
                sourceSize.width: 640
                fillMode: Image.PreserveAspectFit
            }
            Rectangle {
                visible: b.isVideo && picture.shown.status === Image.Ready
                anchors.centerIn: parent
                width: 44
                height: 44
                radius: 22
                color: FiliTheme.alpha(FiliTheme.backgroundDeep, 0.6)
                Text { anchors.centerIn: parent; text: "▶"; font.pixelSize: 18; color: FiliTheme.text }
            }
            Spinner { anchors.centerIn: parent; size: 28; visible: b.fetching && picture.shown.status !== Image.Ready }
            Text {
                anchors.centerIn: parent
                visible: picture.shown.status !== Image.Ready && !b.fetching
                text: b.mediaPath === "" ? (b.expired ? Conversations.tr("No longer on the server: click to ask the phone") : "▣") : ""
                font.family: FiliTheme.font
                font.pixelSize: FiliTheme.sizeList
                color: FiliTheme.muted
            }
            MouseArea { anchors.fill: parent; cursorShape: Qt.PointingHandCursor; onClicked: b.isPicture && b.mediaPath === "" ? b.fetchMedia(true) : win.quickLook(b.message.id) }
        }
        Text {
            visible: b.showsPicture && b.mediaInfo !== ""
            width: parent.width
            text: b.mediaInfo
            elide: Text.ElideRight
            textFormat: Text.PlainText
            font.family: FiliTheme.font
            font.pixelSize: FiliTheme.sizeSmall
            color: FiliTheme.muted
        }
        // Video, document, voice message: a chip (click downloads and opens it)
        Rectangle {
            id: chip
            visible: b.hasMedia && !b.showsPicture
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
                    text: b.fetching ? Conversations.tr("Downloading…") : b.mediaInfo
                    elide: Text.ElideRight
                    font.family: FiliTheme.font
                    font.pixelSize: FiliTheme.sizeSmall
                    color: FiliTheme.muted
                }
            }
            MouseArea { id: chipArea; anchors.fill: parent; hoverEnabled: true; cursorShape: Qt.PointingHandCursor; onClicked: win.quickLook(b.message.id) }
        }
        // YouTube link: thumbnail, title; a click opens it in Quick Look
        Rectangle {
            visible: b.card
            width: parent.width
            height: visible ? width * 9 / 16 + (titleText.implicitHeight > 0 ? titleText.implicitHeight + FiliTheme.space2 : 0) : 0
            radius: FiliTheme.radiusControl
            color: FiliTheme.backgroundDeep
            clip: true
            // The thumbnail plays the video in place when clicked; the button on it opens Quick Look
            VideoLink {
                id: ytThumb
                width: parent.width
                height: width * 9 / 16
                url: b.card ? b.linkUrl : ""
                poster: b.card ? "https://i.ytimg.com/vi/" + b.ytId + "/mqdefault.jpg" : ""
                compact: true
                onExpand: at => win.quickLook(b.message.id, at)
                onPositionChanged: if (position > 0) win.videoPositions[b.message.id] = position
                Connections { target: win; function onStopInline() { ytThumb.pause(); } }
            }
            Text {
                id: titleText
                anchors { left: parent.left; right: parent.right; top: ytThumb.bottom; margins: FiliTheme.space2 }
                visible: b.ytTitle !== ""
                text: b.ytTitle
                maximumLineCount: 2
                wrapMode: Text.Wrap
                elide: Text.ElideRight
                textFormat: Text.PlainText
                font.family: FiliTheme.font
                font.pixelSize: FiliTheme.sizeList
                color: ytLink.containsMouse ? FiliTheme.accentColor : FiliTheme.text
            }
            // Title and source open the page in the default browser
            MouseArea {
                id: ytLink
                anchors { left: parent.left; right: parent.right; top: ytThumb.bottom; bottom: parent.bottom }
                hoverEnabled: true
                cursorShape: Qt.PointingHandCursor
                onClicked: Conversations.openUrl(b.linkUrl)
            }
        }
        Text {
            visible: b.card
            width: parent.width
            text: b.linkSource
            elide: Text.ElideRight
            font.family: FiliTheme.font
            font.pixelSize: FiliTheme.sizeSmall
            color: FiliTheme.muted
        }
        // Page card: image, site, title, description; a click opens the page
        Rectangle {
            visible: b.ogShown
            width: parent.width
            height: visible ? ogColumn.height : 0
            radius: FiliTheme.radiusControl
            color: FiliTheme.alpha(FiliTheme.background, 0.4)
            clip: true
            Column {
                id: ogColumn
                width: parent.width
                // A page that is a video (Instagram, TikTok, Vimeo…): the picture plays it in place
                VideoLink {
                    id: ogVideo
                    visible: b.siteVideo
                    width: parent.width
                    height: visible ? Math.round(width * 9 / 16) : 0
                    url: b.siteVideo ? b.linkUrl : ""
                    poster: b.og.image || ""
                    compact: true
                    onExpand: at => win.quickLook(b.message.id, at)
                    onPositionChanged: if (position > 0) win.videoPositions[b.message.id] = position
                    Connections { target: win; function onStopInline() { ogVideo.pause(); } }
                }
                Image {
                    visible: !b.siteVideo && b.og.image !== undefined && status !== Image.Error
                    width: parent.width
                    height: visible && status === Image.Ready ? Math.round(width * 0.52) : 0
                    source: b.ogShown && b.og.image ? b.og.image : ""
                    fillMode: Image.PreserveAspectCrop
                    asynchronous: true
                    sourceSize.width: 640
                }
                Item {
                    width: parent.width
                    height: ogTexts.height + 2 * FiliTheme.space2
                    Column {
                        id: ogTexts
                        x: FiliTheme.space2
                        y: FiliTheme.space2
                        width: parent.width - 2 * FiliTheme.space2
                        spacing: 2
                        Text {
                            visible: b.og.site !== undefined
                            width: parent.width
                            text: b.og.site || ""
                            elide: Text.ElideRight
                            textFormat: Text.PlainText
                            font.family: FiliTheme.font
                            font.pixelSize: FiliTheme.sizeSmall
                            color: FiliTheme.muted
                        }
                        Text {
                            visible: b.og.title !== undefined
                            width: parent.width
                            text: b.og.title || ""
                            maximumLineCount: 2
                            wrapMode: Text.Wrap
                            elide: Text.ElideRight
                            textFormat: Text.PlainText
                            font.family: FiliTheme.font
                            font.pixelSize: FiliTheme.size
                            font.weight: FiliTheme.weightAccent
                            color: ogLink.containsMouse ? FiliTheme.accentColor : FiliTheme.text
                        }
                        Text {
                            visible: b.og.description !== undefined
                            width: parent.width
                            text: b.og.description || ""
                            maximumLineCount: 3
                            wrapMode: Text.Wrap
                            elide: Text.ElideRight
                            textFormat: Text.PlainText
                            font.family: FiliTheme.font
                            font.pixelSize: FiliTheme.sizeList
                            color: FiliTheme.textSecondary
                        }
                    }
                }
            }
            MouseArea { id: ogLink; anchors.fill: parent; z: -1; hoverEnabled: true; cursorShape: Qt.PointingHandCursor; onClicked: Conversations.openUrl(b.linkUrl) }
        }
        Rectangle {
            visible: b.videoLink
            width: parent.width
            height: visible ? 32 : 0
            radius: FiliTheme.radiusControl
            color: FiliTheme.alpha(FiliTheme.background, 0.4)
            Text { anchors { left: parent.left; leftMargin: FiliTheme.space2; right: parent.right; rightMargin: FiliTheme.space2; verticalCenter: parent.verticalCenter } elide: Text.ElideRight; text: "▶  " + b.linkSource; font.family: FiliTheme.font; font.pixelSize: FiliTheme.sizeList; color: FiliTheme.accentColor }
            MouseArea { anchors.fill: parent; cursorShape: Qt.PointingHandCursor; onClicked: win.quickLook(b.message.id) }
        }
        TextEdit {
            visible: b.body !== "" && !((b.card || b.ogShown) && b.body.trim() === b.linkUrl)
            width: parent.width
            // The text sits apart from the preview above it
            topPadding: b.card || b.ogShown || b.videoLink || b.hasMedia ? FiliTheme.space2 : 0
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
