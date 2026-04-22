"""Create and retrieve a pair of local Phoenix span annotations for P24 discovery."""

from __future__ import annotations

import json
import time

from opentelemetry.exporter.otlp.proto.http.trace_exporter import OTLPSpanExporter
from opentelemetry.sdk.resources import Resource
from opentelemetry.sdk.trace import TracerProvider
from opentelemetry.sdk.trace.export import SimpleSpanProcessor
from openinference.semconv.resource import ResourceAttributes
from phoenix.client import Client


BASE_URL = "http://127.0.0.1:6006"
TRACE_ENDPOINT = f"{BASE_URL}/v1/traces"
PROJECT_NAME = "p24-phoenix-span-annotation"


def _emit_span(span_name: str) -> str:
    resource = Resource.create(
        {
            ResourceAttributes.PROJECT_NAME: PROJECT_NAME,
            "service.name": "p24-phoenix-probe",
        }
    )
    provider = TracerProvider(resource=resource)
    processor = SimpleSpanProcessor(OTLPSpanExporter(endpoint=TRACE_ENDPOINT))
    provider.add_span_processor(processor)
    tracer = provider.get_tracer("p24-phoenix-probe")

    with tracer.start_as_current_span(span_name) as span:
        span.set_attribute("assay.probe", "p24")
        span.set_attribute("assay.surface", "span_annotation")
        span_id = format(span.get_span_context().span_id, "016x")

    provider.force_flush()
    provider.shutdown()
    return span_id


def _wait_for_span(client: Client, span_id: str) -> None:
    for _ in range(20):
        spans = client.spans.get_spans(project_identifier=PROJECT_NAME, limit=100)
        if any(span["context"]["span_id"] == span_id for span in spans):
            return
        time.sleep(0.5)
    raise RuntimeError(f"span {span_id} did not appear in Phoenix")


def main() -> int:
    client = Client(base_url=BASE_URL)

    valid_span_id = _emit_span("p24.annotation.target")
    _wait_for_span(client, valid_span_id)
    valid_create_request = {
        "span_id": valid_span_id,
        "name": "correctness",
        "annotator_kind": "LLM",
        "identifier": "probe-llm-v1",
        "metadata": {"batch": "discovery", "reviewer": "p24-probe"},
        "result": {
            "label": "correct",
            "score": 0.92,
            "explanation": "Bounded probe annotation for P24 discovery.",
        },
    }
    valid_create_response = client.spans.add_span_annotation(
        span_id=valid_span_id,
        annotation_name="correctness",
        annotator_kind="LLM",
        label="correct",
        score=0.92,
        explanation="Bounded probe annotation for P24 discovery.",
        metadata={"batch": "discovery", "reviewer": "p24-probe"},
        identifier="probe-llm-v1",
        sync=True,
    )
    valid_retrieve_response = client.spans.get_span_annotations(
        span_ids=[valid_span_id],
        project_identifier=PROJECT_NAME,
        limit=50,
    )

    failure_span_id = _emit_span("p24.annotation.target.failure")
    _wait_for_span(client, failure_span_id)
    failure_create_request = {
        "span_id": failure_span_id,
        "name": "correctness",
        "annotator_kind": "CODE",
        "result": {
            "label": "incorrect",
            "score": 0.08,
            "explanation": "Bounded negative probe annotation for P24 discovery.",
        },
    }
    failure_create_response = client.spans.add_span_annotation(
        span_id=failure_span_id,
        annotation_name="correctness",
        annotator_kind="CODE",
        label="incorrect",
        score=0.08,
        explanation="Bounded negative probe annotation for P24 discovery.",
        sync=True,
    )
    failure_retrieve_response = client.spans.get_span_annotations(
        span_ids=[failure_span_id],
        project_identifier=PROJECT_NAME,
        limit=50,
    )

    print(
        json.dumps(
            {
                "project_name": PROJECT_NAME,
                "valid": {
                    "create_request": valid_create_request,
                    "create_response": valid_create_response,
                    "retrieve_response": valid_retrieve_response,
                },
                "failure": {
                    "create_request": failure_create_request,
                    "create_response": failure_create_response,
                    "retrieve_response": failure_retrieve_response,
                },
            },
            indent=2,
            sort_keys=True,
        )
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
