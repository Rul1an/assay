import os
import sys
from dataclasses import dataclass, field
from typing import List, Optional
from verdict_sdk import TraceWriter, record_chat_completions, record_chat_completions_with_tools

# --- Tool Executors ---
# Import from local tools.py if available, else define inline
try:
    sys.path.append(os.path.dirname(__file__) + "/openai-demo")
    from tools import GetWeather
except ImportError:
    def GetWeather(args): return {"temp": 22}

# --- Mock OpenAI Client ---
@dataclass
class MockUsage:
    prompt_tokens: int = 10
    completion_tokens: int = 20
    total_tokens: int = 30
    def dict(self): return {"prompt_tokens":10, "completion_tokens":20, "total_tokens":30}

@dataclass
class MockFunction:
    name: str
    arguments: str

@dataclass
class MockToolCall:
    id: str
    function: MockFunction
    type: str = "function"

@dataclass
class MockMessage:
    content: Optional[str]
    tool_calls: Optional[List[MockToolCall]] = None
    role: str = "assistant"
    def dict(self): return {"role": self.role, "content": self.content, "tool_calls": [
        {"id": tc.id, "type": "function", "function": {"name": tc.function.name, "arguments": tc.function.arguments}}
        for tc in (self.tool_calls or [])
    ]}

@dataclass
class MockChoice:
    message: MockMessage

@dataclass
class MockResponse:
    choices: List[MockChoice]
    model: str = "gpt-4o-mini"
    usage: MockUsage = field(default_factory=MockUsage)

class MockCompletions:
    def create(self, **kwargs):
        msgs = kwargs.get("messages", [])
        last_msg = msgs[-1]

        # Check if last msg is tool result -> Final Answer
        if last_msg.get("role") == "tool":
            return MockResponse(choices=[
                MockChoice(message=MockMessage(content="The weather in Tokyo is 22C."))
            ])

        prompt = last_msg.get("content", "") or ""

        if "weather" in prompt.lower():
            # Return Tool Call
            return MockResponse(choices=[
                MockChoice(message=MockMessage(
                    content="",
                    tool_calls=[
                        MockToolCall(
                            id="call_mock_123",
                            function=MockFunction(
                                name="GetWeather",
                                arguments='{"location": "Tokyo"}'
                            )
                        )
                    ]
                ))
            ])
        else:
            return MockResponse(choices=[
                MockChoice(message=MockMessage(content="I am a mock AI."))
            ])

class MockChat:
    completions = MockCompletions()

class MockClient:
    chat = MockChat()

# --- Main Example Flow ---

def main():
    api_key = os.environ.get("OPENAI_API_KEY", "")
    use_mock = api_key == "mock" or not api_key
    mode = os.environ.get("RECORDER_MODE", "simple") # simple | loop

    if use_mock:
        print("Using Mock OpenAI Client")
        client = MockClient()
    else:
        import openai
        client = openai.OpenAI(api_key=api_key)

    trace_path = os.environ.get("VERDICT_TRACE", "traces/openai.jsonl")
    writer = TraceWriter(trace_path)

    # Truncate handled by caller script usually, but verify writer appends.

    messages = [{"role": "user", "content": "What's the weather like in Tokyo?"}]
    tools = [{
        "type": "function",
        "function": {
            "name": "GetWeather",
            "description": "Get current weather",
            "parameters": {"type": "object", "properties": {"location": {"type": "string"}}}
        }
    }]

    if mode == "loop":
        print(f"Recording Loop to {trace_path}...")
        result = record_chat_completions_with_tools(
            writer=writer,
            client=client,
            model="gpt-4o-mini",
            messages=messages,
            tools=tools,
            tool_executors={"GetWeather": GetWeather},
            episode_id="openai_loop_demo",
            test_id="openai_loop_demo",
            prompt=messages[0]["content"]
        )
        print(f"Done Loop. Tool Results: {result['tool_calls']}")
    else:
        print(f"Recording Simple to {trace_path}...")
        result = record_chat_completions(
            writer=writer,
            client=client,
            model="gpt-4o-mini",
            messages=messages,
            tools=tools,
            episode_id="openai_weather_demo",
            test_id="openai_weather_demo",
            prompt=messages[0]["content"]
        )
        print(f"Done Simple.")

if __name__ == "__main__":
    main()
