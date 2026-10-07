# Assay plugin for Claude Code: data handling

This text is based on inspection of the plugin source at `9ebe725d28881673e91104bb67e885bb1f7f6609` and Assay 6.9.0 release source at `61f1adf57302fb8c49ebdbbb29e1ae2adac9694e`. Source inspection does not establish which program or plugin a particular installed host loads. This notice covers the described paths, not every host, configuration or upstream program.

The plugin supplies a skill and configures `assay-mcp-server --policy-root .` as a local MCP server. You install the Assay programs separately. The skill also describes commands run through the host’s shell under its configured permissions; per-command approval prompts are not guaranteed.

## Information processed

Assay tools process the policies, proposed tool arguments, sequence histories and trace information supplied to them. These may contain personal or business information. The default MCP handlers read local policy files and return their evaluation results to the host. Argument checking evaluates a proposed call; it does not perform the named target action.

Results and diagnostics are not a general-purpose anonymization layer. Tool names, identifiers, policy-derived details and error information may appear in responses or logs. The default server sends JSON-RPC results on standard output and diagnostic logs on standard error. Debug logging can additionally include policy paths and cache identifiers. The host or the operator can capture these streams.

`assay doctor` reports local system and diagnostic information, including local counter values and potentially paths or trace-related details. Its checks can start small platform-specific diagnostic processes. The documented JSON invocation does not request fixes or a database; supplying its separate `--db` option can create or migrate local storage.

## Local files

The skill’s CLI commands can create configuration, policies, example traces and evaluation artifacts. The documented run command can write a local SQLite database at `.eval/eval.db`. Its `--db` option changes the database path; `run.json` and `summary.json` are written in the current working directory. Evidence inspection reads the archive you supply and returns information from it to the host.

Do not assume these files expire automatically. Review their contents and manage their storage and deletion according to your requirements. This notice does not promise a fixed deletion schedule. Local execution does not establish how long Claude or another host retains prompts, results or logs; consult the host provider’s applicable terms and product controls.

## Other programs and services

The skill’s proxy example launches a bundled Python mock as a separate local process. Other upstream programs selected by a user can have their own filesystem and network behavior. Local process execution alone does not establish that data is sent to a remote service.

Assay CLI features can send data to external services. In the reviewed source, OpenAI embedding and judge paths use provider credentials and send selected inputs to OpenAI when enabled. The `--embedder openai` command option selects OpenAI embeddings; the evaluation configuration supplies test data, not the embedder provider. The `--judge openai` option or `VERDICT_JUDGE=openai` selects the OpenAI judge. The documented trace replay defaults to no embedder and no judge when environment overrides are absent. Environment settings such as `VERDICT_JUDGE` can change judge selection, and judge-requiring tests can send evaluation inputs and response text to a provider. The generated hello example uses a regex test and does not itself require a live judge call. The provider’s data-handling rules then apply. Do not interpret the local MCP transport as a guarantee that every possible CLI configuration is offline.

The inspected released CLI paths contain tracing instrumentation but no configured OTLP exporter initialization was found for these commands. An endpoint setting alone does not establish that those paths export telemetry. Host log collection and library consumers with their own exporters have separate behavior.

## Contact and scope

For questions about this plugin, contact Roel Schuurkes at roelschuurkes@gmail.com. This notice does not replace the policies of Claude, a selected model provider, an upstream program or an operator’s log collection system.
