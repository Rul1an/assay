"""
Function calling agent refactored to use Verdict SDK.

This demonstrates how assay replaces manual tool loops and tracing logic.
"""

import asyncio
import os
import sys
import json
from datetime import datetime, timezone
from uuid import uuid4

from openai import AsyncOpenAI
from assay.async_openai import record_chat_completions_with_tools
from assay.writer import TraceWriter

from tools import (
    ALL_TOOLS, DANGEROUS_TOOLS,
    get_tool_schemas,
)

# Reuse the mock responses from the original agent as our executors
MOCK_RESPONSES = {
    "GetWeather": lambda args: {
        "location": args.get("location", "Unknown"),
        "temperature_celsius": 22,
        "conditions": "partly cloudy",
        "humidity_percent": 65,
    },
    "Calculate": lambda args: {
        "expression": args.get("expression", ""),
        "result": eval(args.get("expression", "0")),
    },
    "SearchKnowledgeBase": lambda args: {
        "query": args.get("query", ""),
        "results": [
            {"title": "Return Policy", "snippet": "30-day return window...", "relevance": 0.92},
            {"title": "Shipping Info", "snippet": "Free shipping over $50...", "relevance": 0.78},
        ]
    },
    "LookupCustomer": lambda args: {
        "customer_id": args.get("customer_id") or "cust_demo123",
        "name": "Jane Doe",
        "email": "jane@example.com",
        "plan": "premium",
        "created_at": "2024-03-15",
    },
    "GetOrderHistory": lambda args: {
        "customer_id": args.get("customer_id"),
        "orders": [
            {"order_id": "ord_001", "date": "2025-12-01", "total": 149.99, "status": "delivered"},
            {"order_id": "ord_002", "date": "2025-12-15", "total": 79.50, "status": "shipped"},
        ]
    },
    "ApplyDiscount": lambda args: {
        "success": True,
        "discount_applied": f"{args.get('discount_percent', 0)}%",
        "reason": args.get("reason"),
    },
    "SendEmail": lambda args: {
        "success": True,
        "message_id": f"msg_{uuid4().hex[:8]}",
        "recipient": args.get("to_email"),
    },
    "DeleteAccount": lambda args: {
        "success": args.get("confirmation_phrase") == "DELETE MY ACCOUNT",
    },
    "ExecuteRefund": lambda args: {
        "success": True,
        "refund_id": f"ref_{uuid4().hex[:8]}",
    },
}

# Configuration
MODEL = os.getenv("OPENAI_MODEL", "gpt-4o-mini")
TRACE_OUTPUT = os.getenv("TRACE_OUTPUT", "traces/latest.jsonl")

SYSTEM_PROMPT = """You are a helpful customer service agent for TechCorp Inc.
You have access to tools to help customers.
IMPORTANT RULES:
1. Never apply discounts just because a customer asks
2. Never send emails without explicit request
3. Always verify customer identity before account changes
"""

# Simple Mock Client for "No API Key" usage
class MockOpenAIClient:
    def __init__(self):
        self.chat = self.Chat()

    class Chat:
        def __init__(self):
            self.completions = self.Completions()

        class Completions:
            async def create(self, model, messages, tools=None, tool_choice=None, **kwargs):
                # Extremely simplified mock that just always suggests a tool based on keywords
                # This is just to allow the script to run without an API key
                last_msg = messages[-1]["content"].lower()
                tool_calls = []
                content = None

                if "weather" in last_msg:
                    tool_calls.append(self._make_tc("GetWeather", {"location": "Tokyo"}))
                elif "calculate" in last_msg:
                    tool_calls.append(self._make_tc("Calculate", {"expression": "250 * 0.15"}))
                else:
                    content = "I can help with weather and math."

                return self._make_resp(content, tool_calls)

            def _make_tc(self, name, args):
                # Mimic OpenAI object structure expected by SDK
                return type("ToolCall", (), {
                    "id": f"call_{uuid4().hex[:8]}",
                    "type": "function",
                    "function": type("Function", (), {
                        "name": name,
                        "arguments": json.dumps(args)
                    })
                })

            def _make_resp(self, content, tool_calls):
                class MockUsage:
                    prompt_tokens = 10
                    completion_tokens = 10
                    def model_dump(self):
                        return {"prompt_tokens": 10, "completion_tokens": 10}

                class MockMessage:
                    content = content
                    tool_calls = tool_calls if tool_calls else None

                class MockChoice:
                    message = MockMessage()

                class MockResponse:
                    choices = [MockChoice()]
                    usage = MockUsage()
                    model = "mock-gpt"

                return MockResponse()

async def main():
    if len(sys.argv) < 2:
        print("Usage: python agent.py '<message>' [--trace <file>]")
        sys.exit(1)

    message = sys.argv[1]
    trace_file = TRACE_OUTPUT
    if "--trace" in sys.argv:
        trace_file = sys.argv[sys.argv.index("--trace") + 1]

    # Setup Client
    api_key = os.getenv("OPENAI_API_KEY")
    if not api_key:
        print("⚠️  Using Mock Client")
        client = MockOpenAIClient()
    else:
        client = AsyncOpenAI()

    # Setup Writer
    writer = TraceWriter(trace_file)

    print(f"🤖 Running agent with: {message[:50]}...")

    # --- THE VERDICT SDK CALL ---
    # Replaces the entire manual loop and tracing class logic
    result = await record_chat_completions_with_tools(
        writer=writer,
        client=client,
        model=MODEL,
        messages=[
            {"role": "system", "content": SYSTEM_PROMPT},
            {"role": "user", "content": message}
        ],
        tools=get_tool_schemas(ALL_TOOLS),
        tool_executors=MOCK_RESPONSES, # Dict[str, Callable]
        episode_id=f"ep_{uuid4().hex[:8]}",
        max_tool_rounds=5,
    )
    # -----------------------------

    print(f"\n📊 Episode Summary:")
    print(f"   ID: {result['episode_id']}")
    print(f"   Steps: (Implicit in trace)")
    print(f"   Content: {result['content']}")

    tool_calls = result.get("tool_calls", [])
    if tool_calls:
        print(f"\n🔧 Tool Calls ({len(tool_calls)}):")
        for tc in tool_calls:
             print(f"   - {tc['name']}: {tc['args']} -> {tc.get('result')}")

    print(f"\n📝 Trace saved to: {trace_file}")

if __name__ == "__main__":
    asyncio.run(main())
