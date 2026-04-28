import type { ExportedScore, ScoreEvent } from '@mastra/core/observability';

type AssayScoreArtifact = {
  schema: 'mastra.score-event.export.v1';
  framework: 'mastra';
  surface: 'observability.score_event';
  timestamp: string;
  score_id_ref?: string;
  scorer_id?: string;
  scorer_name?: string;
  score: number;
  target_ref: string;
  target_entity_type?: string;
  reason?: string;
  trace_id_ref?: string;
  span_id_ref?: string;
  scorer_version?: string;
  score_source?: string;
  metadata_ref?: string;
};

/**
 * Tiny illustrative sketch of the primary exporter seam we care about.
 *
 * This is not a production adapter. It shows the typed `onScoreEvent` path
 * that Mastra now points external score consumers toward.
 *
 * A legacy `addScoreToTrace` sketch is left below only as migration context
 * because Mastra has explicitly called that pathway old and pending
 * deprecation.
 */
export class AssayScoreCaptureExporter {
  readonly scores: ExportedScore[] = [];

  async onScoreEvent(event: ScoreEvent): Promise<void> {
    this.scores.push(event.score);
  }

  drainArtifacts(): AssayScoreArtifact[] {
    const drained = this.scores.map(toAssayScoreArtifact);
    this.scores.length = 0;
    return drained;
  }
}

/**
 * Historical / transitional sketch only.
 *
 * Keep this only to show why older docs and code samples may still mention a
 * thinner score-attach payload. It is not the target seam for P14 anymore.
 */
export class AssayLegacyScoreAttachExporter {
  readonly scores: AssayScoreArtifact[] = [];

  async addScoreToTrace(args: {
    traceId: string;
    spanId?: string;
    score: number;
    reason?: string;
    scorerName: string;
    metadata?: Record<string, unknown>;
  }): Promise<void> {
    this.scores.push({
      schema: 'mastra.score-event.export.v1',
      framework: 'mastra',
      surface: 'observability.score_event',
      timestamp: new Date().toISOString(),
      scorer_name: args.scorerName,
      score: args.score,
      target_ref: args.spanId ?? args.traceId,
      ...(args.reason ? { reason: args.reason } : {}),
      ...(args.traceId ? { trace_id_ref: args.traceId } : {}),
      ...(args.spanId ? { span_id_ref: args.spanId } : {}),
      ...(args.metadata ? { metadata_ref: 'metadata:redacted' } : {}),
    });
  }

  drainArtifacts(): AssayScoreArtifact[] {
    const drained = [...this.scores];
    this.scores.length = 0;
    return drained;
  }
}

type ForwardCompatibleExportedScore = ExportedScore & {
  scoreId?: string;
};

function normalizeOpaqueRef(value: unknown): string | undefined {
  if (typeof value !== 'string') {
    return undefined;
  }

  const normalized = value.trim();
  if (!normalized) {
    return undefined;
  }

  const opaqueRefPattern = /^[A-Za-z0-9:_\-.]+$/;
  if (!opaqueRefPattern.test(normalized) || normalized.includes('://')) {
    throw new Error('Score event is missing a bounded opaque score id');
  }

  return normalized;
}

function normalizeClassifier(value: unknown): string | undefined {
  if (value == null) {
    return undefined;
  }

  const normalized = String(value)
    .trim()
    .replace(/([a-z0-9])([A-Z])/g, '$1_$2')
    .replace(/\s+/g, '_')
    .toLowerCase()
    .replace(/[^a-z0-9_-]/g, '_')
    .replace(/_+/g, '_')
    .replace(/^[_-]+|[_-]+$/g, '');

  if (!normalized) {
    return undefined;
  }

  return normalized;
}

function normalizeScoreSource(value: unknown): AssayScoreArtifact['score_source'] | undefined {
  const normalized = normalizeClassifier(value);
  if (!normalized) {
    return undefined;
  }

  if (normalized === 'live' || normalized === 'trace' || normalized === 'experiment') {
    return normalized;
  }

  return undefined;
}

function toAssayScoreArtifact(score: ExportedScore): AssayScoreArtifact {
  const forwardScore = score as ForwardCompatibleExportedScore;
  const targetRef = score.spanId ?? score.traceId ?? score.correlationContext?.entityId;
  if (!targetRef) {
    throw new Error('Score event is missing a bounded target anchor');
  }

  const scorerId = score.scorerId;
  const scorerName = score.scorerName;
  if (!scorerId && !scorerName) {
    throw new Error('Score event is missing a scorer identity');
  }

  const scoreIdRef = normalizeOpaqueRef(forwardScore.scoreId);
  const targetEntityType = normalizeClassifier(score.targetEntityType);
  const scoreSource = normalizeScoreSource(score.scoreSource);

  return {
    schema: 'mastra.score-event.export.v1',
    framework: 'mastra',
    surface: 'observability.score_event',
    timestamp: score.timestamp.toISOString(),
    ...(scoreIdRef ? { score_id_ref: scoreIdRef } : {}),
    ...(scorerId ? { scorer_id: scorerId } : {}),
    ...(scorerName ? { scorer_name: scorerName } : {}),
    score: score.score,
    target_ref: targetRef,
    ...(targetEntityType ? { target_entity_type: targetEntityType } : {}),
    ...(score.reason ? { reason: score.reason } : {}),
    ...(score.traceId ? { trace_id_ref: score.traceId } : {}),
    ...(score.spanId ? { span_id_ref: score.spanId } : {}),
    ...(score.scorerVersion ? { scorer_version: score.scorerVersion } : {}),
    ...(scoreSource ? { score_source: scoreSource } : {}),
    ...(score.metadata ? { metadata_ref: 'metadata:redacted' } : {}),
  };
}
