# Scripts and directions

## Bangla

কিতাব একটি দ্রুত, হালকা পাঠক। যুক্তাক্ষর ঠিকভাবে দেখাতে হবে: ক্ষ, জ্ঞ, ন্ত্র, স্ক্র।

## Arabic (right to left)

<div dir="rtl">

كتاب قارئ سريع وخفيف. يجب أن يبدأ هذا السطر من اليمين، وأن تتصل الحروف ببعضها.

- عنصر أول
- عنصر ثانٍ

</div>

Mixed direction in one line: the word كتاب means book, and the number 2026 stays left to right.

## Hebrew

ספר הוא קורא מהיר.

## Chinese, Japanese, Korean

中文：快速、轻量的阅读器。
日本語：速くて軽いリーダーです。
한국어: 빠르고 가벼운 리더입니다.

## Emoji and combining marks

Emoji with modifiers and joiners: 👩🏽‍💻 🏳️‍🌈 👨‍👩‍👧‍👦. Combining marks: é (e + U+0301), ñ, Å.

## Unicode bullets

Text pasted from chat apps uses bullet glyphs. Kitab must turn these into a real list, as md-viewer-pwa's `normalizeMarkdown` does, but leave code fences alone:

• first pasted bullet
• second pasted bullet
  ◦ nested pasted bullet
▸ arrow bullet

```text
• this bullet is inside a code fence and must stay as typed
```

## Search tests

Searching for "কিতাব", "كتاب" and "快速" must each find a match on this page. Searching "cafe" with diacritic-insensitive matching on finds "café".
