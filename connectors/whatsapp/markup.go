// WhatsApp markup ↔ neutral text spans: *bold*, _italic_, ~strike~, `code`, ```pre```.
package main

import (
	"sort"
	"strings"
	"unicode"
)

var styleOf = map[rune]string{'*': "bold", '_': "italic", '~': "strike", '`': "code"}
var markerOf = map[string]string{"bold": "*", "italic": "_", "strike": "~", "code": "`", "pre": "```"}

// A marker opens at the start, or after a space or punctuation, before a non-space; it closes after a non-space,
// before the end, a space or punctuation. Markers never span lines (except ```).
func opens(r []rune, i int) bool {
	if i+1 >= len(r) || unicode.IsSpace(r[i+1]) {
		return false
	}
	return i == 0 || unicode.IsSpace(r[i-1]) || isBoundaryPunct(r[i-1])
}

func closes(r []rune, i int) bool {
	if i == 0 || unicode.IsSpace(r[i-1]) {
		return false
	}
	return i+1 == len(r) || unicode.IsSpace(r[i+1]) || isBoundaryPunct(r[i+1])
}

func isBoundaryPunct(c rune) bool {
	if _, marker := styleOf[c]; marker {
		return true
	}
	return unicode.IsPunct(c) && c != '_' || unicode.IsSymbol(c)
}

// ParseMarkup turns WhatsApp markup into plain text and spans (offsets in runes).
func ParseMarkup(s string) Text {
	if !strings.ContainsAny(s, "*_~`") {
		return Text{Plain: s}
	}
	plain, spans := parse([]rune(s))
	return Text{Plain: string(plain), Spans: spans}
}

func parse(r []rune) ([]rune, []Span) {
	var out []rune
	var spans []Span
	for i := 0; i < len(r); i++ {
		// ```pre``` first: it may cross lines and holds no markup.
		if r[i] == '`' && i+2 < len(r) && r[i+1] == '`' && r[i+2] == '`' {
			if end := indexRunes(r, i+3, []rune("```")); end > i+3 {
				start := len(out)
				out = append(out, r[i+3:end]...)
				spans = append(spans, Span{Start: start, End: len(out), Style: "pre"})
				i = end + 2
				continue
			}
		}
		style, isMarker := styleOf[r[i]]
		if isMarker && opens(r, i) {
			if end := closingIndex(r, i); end > 0 {
				inner := r[i+1 : end]
				start := len(out)
				if style == "code" {
					out = append(out, inner...)
				} else {
					innerPlain, innerSpans := parse(inner)
					for _, sp := range innerSpans {
						sp.Start += start
						sp.End += start
						spans = append(spans, sp)
					}
					out = append(out, innerPlain...)
				}
				spans = append(spans, Span{Start: start, End: len(out), Style: style})
				i = end
				continue
			}
		}
		out = append(out, r[i])
	}
	sort.SliceStable(spans, func(a, b int) bool { return spans[a].Start < spans[b].Start })
	return out, spans
}

func closingIndex(r []rune, open int) int {
	for j := open + 2; j < len(r); j++ {
		if r[j] == '\n' {
			return -1
		}
		if r[j] == r[open] && closes(r, j) {
			return j
		}
	}
	return -1
}

func indexRunes(r []rune, from int, needle []rune) int {
	for i := from; i+len(needle) <= len(r); i++ {
		match := true
		for k := range needle {
			if r[i+k] != needle[k] {
				match = false
				break
			}
		}
		if match {
			return i
		}
	}
	return -1
}

// FormatMarkup turns neutral text back into WhatsApp markup. Links and mentions stay plain text; spans that cross
// each other are dropped rather than producing broken markup.
func FormatMarkup(t Text) string {
	if len(t.Spans) == 0 {
		return t.Plain
	}
	r := []rune(t.Plain)
	type mark struct {
		pos   int
		open  bool
		width int
		index int
		text  string
	}
	var kept []Span
	for _, sp := range t.Spans {
		m, ok := markerOf[sp.Style]
		if !ok || m == "" || sp.Start < 0 || sp.End > len(r) || sp.Start >= sp.End {
			continue
		}
		crosses := false
		for _, k := range kept {
			if sp.Start < k.End && k.Start < sp.End && !(sp.Start >= k.Start && sp.End <= k.End) && !(k.Start >= sp.Start && k.End <= sp.End) {
				crosses = true
				break
			}
		}
		if !crosses {
			kept = append(kept, sp)
		}
	}
	var marks []mark
	for i, sp := range kept {
		m := markerOf[sp.Style]
		marks = append(marks, mark{sp.Start, true, sp.End - sp.Start, i, m}, mark{sp.End, false, sp.End - sp.Start, i, m})
	}
	// At one position: closings before openings; inner closes first, outer opens first. Spans of equal extent nest
	// in list order (the parser lists the inner one first).
	sort.SliceStable(marks, func(a, b int) bool {
		x, y := marks[a], marks[b]
		if x.pos != y.pos {
			return x.pos < y.pos
		}
		if x.open != y.open {
			return !x.open
		}
		if x.width != y.width {
			if x.open {
				return x.width > y.width
			}
			return x.width < y.width
		}
		if x.open {
			return x.index > y.index
		}
		return x.index < y.index
	})
	var b strings.Builder
	k := 0
	for i := 0; i <= len(r); i++ {
		for k < len(marks) && marks[k].pos == i {
			b.WriteString(marks[k].text)
			k++
		}
		if i < len(r) {
			b.WriteRune(r[i])
		}
	}
	return b.String()
}
