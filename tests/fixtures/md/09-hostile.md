# Hostile input

Nothing on this page may run code, load remote content without asking, or crash the renderer. The page must still render the safe text.

<script>window.pwned = true</script>

<img src="x" onerror="window.pwned = true">

<iframe src="https://example.invalid/"></iframe>

<a href="javascript:alert(1)">javascript link</a> must not run.

[markdown javascript link](javascript:alert(1)) must not run.

[data URL link](data:text/html;base64,PHNjcmlwdD5hbGVydCgxKTwvc2NyaXB0Pg==) must not run.

<svg><script>alert(1)</script><circle r="10"/></svg>

<style>body { display: none }</style>

![Remote image: blocked until the user allows remote images](https://example.invalid/tracker.png)

<div style="position:fixed;inset:0;background:red">Overlay that must not cover the app</div>

Deep nesting:

> > > > > > > > > > > > > > > > > > > > > > > > > > > > > > > > > > > > > > > > deep quote

- - - - - - - - - - - - - - - - - - - - - - - - - - - - - - - - - - - - - - deep list

A zero-width joiner flood: ‍‍‍‍‍‍‍‍‍‍‍‍‍‍‍‍‍‍‍‍‍‍‍‍‍‍‍‍‍‍‍‍‍‍‍‍‍‍‍‍ end.

Unbalanced emphasis ***** ___ ** _ * and an unclosed fence follow.

```
an unclosed code fence runs to the end of the file
