# Assay for Claude Code

Assay provides local MCP tools for policy checks, coverage and trace explanations, plus a skill for its CLI install-to-evidence workflow. Install the Assay CLI and `assay-mcp-server` using the installation documentation before using this plugin. Both executables must be available to the process that starts Claude Code. The plugin does not install those executables.

The bundled skill describes nine steps, their documented outputs, exit codes and known limitations. It includes fixtures for a local demonstration. Read the skill and its machine contract before executing a step; commands can create starter files or other local output. Behavior available only after the published release is explicitly marked Unreleased in the contract.

The MCP server starts locally with `--policy-root .`. Choose the working directory and policy inputs deliberately. A policy denial, a successful tool exchange and a process exit code are different observations. Evidence integrity verification does not prove an external action occurred.

Policy files, supplied arguments and traces can contain personal or business data. Evaluation results return to the host; diagnostics and decision events can be logged to standard error. Local execution does not determine host retention. Separate skill-triggered CLI commands can write artifacts or invoke upstream programs; inspect each command before running it.

This package targets Claude Code and requires local executables and files. Its MCP server starts as a local command, not a hosted endpoint. Installing the skill on another surface does not by itself provide those executables or a running MCP server.

Installation documentation: https://github.com/Rul1an/assay/blob/main/docs/getting-started/installation.md
Project and support: https://github.com/Rul1an/assay
Publisher: Rul1an (Roel Schuurkes).
Support contact: roelschuurkes@gmail.com
Security policy: https://github.com/Rul1an/assay/blob/main/SECURITY.md
License: MIT, included in this folder.
