# Kitchen sink

Every GitHub Flavored Markdown element Kitab must render. Each section names what to check.

## Headings

# H1 heading
## H2 heading
### H3 heading
#### H4 heading
##### H5 heading
###### H6 heading

Setext H1
=========

Setext H2
---------

## Inline formatting

Plain, **bold**, *italic*, ***bold italic***, ~~strikethrough~~, `inline code`, and a [link to the Markdown spec](https://spec.commonmark.org/).
Autolink: https://kitab.example.invalid/docs and <mailto:reader@example.invalid>.
Hard line break with two trailing spaces  
and a backslash\
then the next line.

Escaped characters: \*not italic\*, \# not a heading, \`not code\`.

## Lists

- Unordered item
- Second item
  - Nested item
    - Third level
- Back to the top

1. Ordered item
2. Second
   1. Nested ordered
   2. Another
10. Number jumps to ten

7. A list that starts at seven
8. Eight

### Task list

- [x] Open a PDF
- [x] Open an EPUB
- [ ] Open a Markdown file with diagrams
- [ ] Sign a PDF

## Blockquotes

> A blockquote.
>
> > A nested blockquote.
> >
> > - with a list
>
> Back to the first level.

## Table

| Format | Engine | Reflowable | Text search |
| :----- | :----: | ---------: | ----------- |
| PDF | MuPDF | no | yes |
| EPUB | MuPDF | yes | yes |
| Markdown | native | yes | yes |
| CBZ | MuPDF | no | no |

A table wider than the reading column must scroll sideways inside its own box:

| a | b | c | d | e | f | g | h | i | j | k | l | m | n | o | p |
|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|
| a-long-cell-value | b-long-cell-value | c-long-cell-value | d-long-cell-value | e-long-cell-value | f-long-cell-value | g-long-cell-value | h-long-cell-value | i | j | k | l | m | n | o | p |

## Code

```rust
fn main() {
    println!("Hello from Kitab");
}
```

## Footnotes

Kitab renders footnotes as clickable references.[^1] A second note uses a named label.[^reader]

[^1]: The first footnote.
[^reader]: A named footnote with **formatting** and a [link](https://example.invalid).

## Images

![A local PNG that exists](assets/sample.png)

![Alt text shown when the image is missing](assets/does-not-exist.png)

## Horizontal rule

---

## HTML that Markdown allows

<details>
<summary>Click to expand</summary>

Hidden content inside a details element.

</details>

Keyboard: <kbd>Ctrl</kbd> + <kbd>F</kbd>. Subscript H<sub>2</sub>O and superscript x<sup>2</sup>.

## Definition-like and edge cases

Term with no blank line after
: is not a definition list in GFM, so this renders as a paragraph.

A paragraph with a very long word that must wrap inside the column instead of pushing it wider: Pneumonoultramicroscopicsilicovolcanoconiosis-and-then-some-more-characters-to-be-sure.
