import json
import uuid
import time
import sys

# ANSI Colors
CYAN = "\033[96m"
GREEN = "\033[92m"
YELLOW = "\033[93m"
RESET = "\033[0m"
BOLD = "\033[1m"

def log(msg, color=RESET):
    # Print to stderr so it doesn't pollute the JSONL output on stdout
    sys.stderr.write(f"{color}{msg}{RESET}\n")

TRACE_ID = uuid.uuid4().hex

def current_time_nanos():
    return int(time.time() * 1_000_000_000)

def write_span(name, start_time, end_time, parent_span_id, attributes):
    span_id = uuid.uuid4().hex
    span = {
        "name": name,
        "context": {
            "trace_id": TRACE_ID,
            "span_id": span_id,
        },
        "parent_id": parent_span_id,
        "start_time_unix_nano": start_time,
        "end_time_unix_nano": end_time,
        "attributes": attributes
    }

    flat_span = {
        "trace_id": TRACE_ID,
        "span_id": span_id,
        "parent_span_id": parent_span_id,
        "name": name,
        "startTimeUnixNano": str(start_time),
        "endTimeUnixNano": str(end_time),
        "attributes": attributes
    }
    print(json.dumps(flat_span))
    return span_id

def main():
    # Simulate a run: User asks "What's the weather in Paris?"
    # Agent calls tool "get_weather".
    # Agent replies "It's sunny".

    start_run = current_time_nanos()

    log(f"{BOLD}🤖 Agent Started{RESET} (Trace ID: {TRACE_ID[:8]}...)")
    log(f"{CYAN}User:{RESET} What's the weather in Paris?")

    # 1. Root Span (The Agent Interaction)
    # FIX: Use "invoke_agent" or similar so it's not confused with a model step
    root_attributes = {
        "gen_ai.operation.name": "invoke_agent",
        "gen_ai.system": "You are a helpful assistant.",
        "gen_ai.request.model": "gpt-4o-mini",
    }

    # Start Root
    root_start = current_time_nanos()

    # 2. Step 1: Model "Thinking" / Call
    log(f"{YELLOW}Thinking...{RESET}")
    step1_start = current_time_nanos()
    time.sleep(0.5)
    step1_end = current_time_nanos()

    # FIX: Add gen_ai.operation.name = chat
    step1_attributes = {
        "gen_ai.operation.name": "chat",
        "gen_ai.system": "You are a helpful assistant.",
        "gen_ai.prompt": "What's the weather in Paris?",
        "gen_ai.completion": "",
    }

    write_span("chat.completion", step1_start, step1_end, None, step1_attributes)

    # 3. Tool Execution Span
    log(f"{GREEN}🛠️  Call Tool:{RESET} get_weather(location='Paris')")
    tool_start = current_time_nanos()
    time.sleep(0.3)
    tool_end = current_time_nanos()

    tool_attributes = {
        "gen_ai.operation.name": "execute_tool",
        "gen_ai.tool.name": "get_weather",
        "gen_ai.tool.args": json.dumps({"location": "Paris"}),
        "gen_ai.tool.result": json.dumps({"condition": "Sunny", "temp": 24}),
    }
    write_span("tool_execution", tool_start, tool_end, None, tool_attributes)

    # 4. Step 2: Model "Final Answer"
    log(f"{YELLOW}Thinking...{RESET}")
    step2_start = current_time_nanos()
    time.sleep(0.2)
    step2_end = current_time_nanos()

    final_answer = "The weather in Paris is sunny and 24°C."
    log(f"{CYAN}Agent:{RESET} {final_answer}")

    # FIX: Add gen_ai.operation.name = chat
    step2_attributes = {
        "gen_ai.operation.name": "chat",
        "gen_ai.prompt": "",
        "gen_ai.completion": final_answer,
        "gen_ai.response.model": "gpt-4o-mini"
    }
    write_span("chat.completion", step2_start, step2_end, None, step2_attributes)

    # Root Span End
    root_attributes["gen_ai.completion"] = final_answer
    root_end = current_time_nanos()

    write_span("agent_run", root_start, root_end, None, root_attributes)

if __name__ == "__main__":
    main()
