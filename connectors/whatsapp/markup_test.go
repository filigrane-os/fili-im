package main

import (
	"reflect"
	"testing"
)

func TestParseMarkup(t *testing.T) {
	cases := []struct {
		in   string
		want Text
	}{
		{"plain", Text{Plain: "plain"}},
		{"*gras* et _italique_", Text{Plain: "gras et italique", Spans: []Span{{0, 4, "bold", "", ""}, {8, 16, "italic", "", ""}}}},
		{"~barré~", Text{Plain: "barré", Spans: []Span{{0, 5, "strike", "", ""}}}},
		{"`a*b*`", Text{Plain: "a*b*", Spans: []Span{{0, 4, "code", "", ""}}}},
		{"```x\n*y*```", Text{Plain: "x\n*y*", Spans: []Span{{0, 5, "pre", "", ""}}}},
		{"*_les deux_*", Text{Plain: "les deux", Spans: []Span{{0, 8, "italic", "", ""}, {0, 8, "bold", "", ""}}}},
		{"2*3*4", Text{Plain: "2*3*4"}},
		{"* pas gras *", Text{Plain: "* pas gras *"}},
		{"snake_case_name", Text{Plain: "snake_case_name"}},
		{"*sur\ndeux lignes*", Text{Plain: "*sur\ndeux lignes*"}},
		{"(*ok*)", Text{Plain: "(ok)", Spans: []Span{{1, 3, "bold", "", ""}}}},
	}
	for _, c := range cases {
		got := ParseMarkup(c.in)
		if got.Plain != c.want.Plain || !reflect.DeepEqual(nonNil(got.Spans), nonNil(c.want.Spans)) {
			t.Errorf("ParseMarkup(%q) = %+v, want %+v", c.in, got, c.want)
		}
	}
}

func nonNil(s []Span) []Span {
	if s == nil {
		return []Span{}
	}
	return s
}

func TestFormatMarkupRoundTrips(t *testing.T) {
	for _, in := range []string{"*gras* et _italique_", "~barré~", "`code`", "```x\ny```", "*_les deux_*", "rien"} {
		if out := FormatMarkup(ParseMarkup(in)); out != in {
			t.Errorf("round trip %q → %q", in, out)
		}
	}
	link := Text{Plain: "voir ici", Spans: []Span{{Start: 5, End: 8, Style: "link", URL: "https://x"}}}
	if out := FormatMarkup(link); out != "voir ici" {
		t.Errorf("links stay plain: %q", out)
	}
	crossing := Text{Plain: "abcdef", Spans: []Span{{Start: 0, End: 4, Style: "bold"}, {Start: 2, End: 6, Style: "italic"}}}
	if out := FormatMarkup(crossing); out != "*abcd*ef" {
		t.Errorf("crossing spans: %q", out)
	}
}
