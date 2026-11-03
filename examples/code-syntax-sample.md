# Syntax highlighting

MiraMD keeps fenced code as standard Markdown and colors it locally in Preview and Visual modes.

```rust
fn main() {
    let greeting = "ສະບາຍດີ";
    println!("{greeting}, MiraMD!");
}
```

```javascript
const documents = ["README.md", "SKILL.md"];
documents.forEach((name) => console.log(`Opening ${name}`));
```

```json
{
  "language": "lo-LA",
  "export": "docx",
  "highlight": true
}
```

An unknown language remains readable and safely escaped:

```made-up
<tag>& value
```
