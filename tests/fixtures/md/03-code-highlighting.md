# Code blocks

Each block needs syntax colours in every theme, a copy button, and sideways scrolling for long lines. Blocks with no language or an unknown one render as plain monospace.

```rust
use std::collections::HashMap;

/// A tile key: page index, zoom bucket, tile x/y.
#[derive(Hash, Eq, PartialEq, Clone, Copy)]
pub struct TileKey { page: u32, zoom: u16, x: u16, y: u16 }

pub fn evict(cache: &mut HashMap<TileKey, Vec<u8>>, budget: usize) {
    while cache.values().map(Vec::len).sum::<usize>() > budget {
        let key = *cache.keys().next().expect("non-empty");
        cache.remove(&key);
    }
}
```

```typescript
type Theme = "dark" | "light" | "paper";
export const next = (t: Theme): Theme => (t === "dark" ? "light" : t === "light" ? "paper" : "dark");
```

```python
def reading_time(words: int, wpm: int = 230) -> str:
    minutes = max(1, round(words / wpm))
    return f"{minutes} min"
```

```go
package main

import "fmt"

func main() { fmt.Println("kitab") }
```

```sql
SELECT d.title, r.page, r.updated_at
FROM documents d
JOIN reading_state r ON r.document_id = d.id
ORDER BY r.updated_at DESC
LIMIT 10;
```

```bash
cargo run --release -- --open "tests/fixtures/pdf/01-text-outline.pdf"
```

```powershell
Get-ChildItem tests\fixtures -Recurse -Include *.pdf | Measure-Object Length -Sum
```

```json
{ "theme": "paper", "density": "compact", "icons": "hairline" }
```

```yaml
keymap:
  ctrl-f: find::Open
  ctrl-g: navigate::GoToPage
```

```toml
[package]
name = "kitab"
edition = "2024"
```

```diff
- let zoom = 1.0;
+ let zoom = settings.default_zoom;
```

```html
<p class="note">Rendered as code, never as HTML.</p>
<script>alert("must not run")</script>
```

```
A fence with no language.
```

```not-a-real-language
A fence with an unknown language falls back to plain text.
```

```text
A single very long line that must scroll sideways inside the block instead of widening the page: 0123456789 0123456789 0123456789 0123456789 0123456789 0123456789 0123456789 0123456789 0123456789 0123456789 0123456789 0123456789
```

    An indented code block (four spaces), no fence.

~~~python
# A tilde fence works too
print("tilde")
~~~
