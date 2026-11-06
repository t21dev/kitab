# Mermaid diagrams

Every block below is a ` ```mermaid ` fence. Each must render as a diagram in all three themes. The last one is deliberately invalid and must show an inline error with the source, not crash the document.

## Flowchart

```mermaid
flowchart LR
    Open[Open file] --> Detect{Format?}
    Detect -->|PDF / XPS / CBZ| MuPDF[MuPDF page renderer]
    Detect -->|EPUB / MOBI / FB2| Reflow[MuPDF reflow layout]
    Detect -->|Markdown| Native[Native GPUI renderer]
    MuPDF --> Tiles[Tile cache]
    Reflow --> Tiles
    Native --> View[Reader view]
    Tiles --> View
```

## Sequence diagram

```mermaid
sequenceDiagram
    actor Reader
    participant UI as GPUI window
    participant W as Render worker
    participant DB as SQLite
    Reader->>UI: Ctrl+O, pick file
    UI->>W: open(path)
    W-->>UI: page count, outline
    UI->>DB: restore position
    DB-->>UI: page 24, zoom 110%
    W-->>UI: low-res preview
    W-->>UI: sharp tiles
```

## Class diagram

```mermaid
classDiagram
    class Document {
        +String path
        +Format format
        +page_count() u32
    }
    class PdfDocument
    class EpubDocument
    class MarkdownDocument
    Document <|-- PdfDocument
    Document <|-- EpubDocument
    Document <|-- MarkdownDocument
```

## State diagram

```mermaid
stateDiagram-v2
    [*] --> Unsigned
    Unsigned --> Signing: Sign document
    Signing --> Signed: key unlocked
    Signing --> Unsigned: cancel
    Signed --> Modified: edit after signing
    Modified --> [*]
```

## Entity relationship

```mermaid
erDiagram
    DOCUMENT ||--o{ BOOKMARK : has
    DOCUMENT ||--o{ HIGHLIGHT : has
    HIGHLIGHT ||--o| NOTE : carries
    DOCUMENT ||--|| READING_STATE : remembers
```

## Gantt

```mermaid
gantt
    title Kitab v1
    dateFormat YYYY-MM-DD
    section Core
    Spike GPUI + MuPDF     :a1, 2026-11-09, 10d
    PDF reader             :a2, after a1, 20d
    section Formats
    EPUB                   :b1, after a2, 15d
    Markdown               :b2, after a2, 12d
```

## Pie

```mermaid
pie title Library by format
    "PDF" : 62
    "EPUB" : 25
    "Markdown" : 13
```

## Mindmap

```mermaid
mindmap
  root((Kitab))
    Read
      PDF
      EPUB
      Markdown
    Annotate
      Highlights
      Notes
    Sign
      Verify
      Apply
```

## Timeline

```mermaid
timeline
    title Reading session
    09:00 : Opened Meditations
    09:20 : Highlighted Book II
    09:45 : Exported highlights
```

## Git graph

```mermaid
gitGraph
    commit
    branch feature
    checkout feature
    commit
    checkout main
    merge feature
```

## Invalid diagram (must fail gracefully)

```mermaid
flowchart LR
    A --> 
    this is not valid mermaid ((((
```

## A normal code block that only mentions mermaid

```text
mermaid is not the language of this block, so it stays plain text
```
