# Shields API

A high-performance web API for generating shield badges compatible with shields.io, built with Rust and Axum.

## Shields API

Generate shield badges from any JSON API endpoint.

**Usage Example:**

```bash
curl "http://localhost:1581/endpoint?url=https://api.github.com/repos/microsoft/vscode&query=stargazers_count&label=stars&color=green"
```

**Parameters:**

- `url` (required): API endpoint URL
- `query`: JSONPath query (e.g. `version`, `data.count`)
- `label`: Left text
- `color`: Badge color (named or hex)
- `style`: Badge style (flat, plastic, etc.)

**Response:** SVG badge (`image/svg+xml`)

## License

[MIT License](LICENSE)
