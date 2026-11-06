# MCP server

Run `onde` as a [Model Context Protocol](https://modelcontextprotocol.io/) server over stdio instead of the TUI:

```sh
onde --mcp
```

This exposes Onde account and model-catalog operations as MCP tools — `login`, `me`, `apps_list`, `app_create`, `app_rename`, `models_list`, `model_register`, `model_assign`, `hf_search` — returning structured JSON. stdout is the JSON-RPC channel; tools run non-interactively and reuse the token from a TUI sign-in (or the `login` tool). Point any MCP client at the command `onde --mcp`.

The listing in the official MCP Registry is covered in [mcp-registry.md](mcp-registry.md).
