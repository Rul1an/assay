import type { ExportedScore, ScoreEvent } from '@mastra/core/observability';

type AssayScoreArtifact = {
  schema: 'mastra.score-event.export.v1';
  framework: 'mastra';
  surface: 'observability_score_event';
  timestamp: string;
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
 * Tiny illustrative sketch of the two adjacent exporter seams we care about.
 *
 * This is not a production adapter. It shows:
 * - the richer typed `onScoreEvent` path described by Mastra's score-event types
 * - the currently wired `addScoreToTrace` path used by the scorer hook
 *
 * The second path is important because it is thinner: it currently drops
 * fields such as `scorerId` and `targetEntityType`, so an external consumer
 * should not assume those are always present until a real capture confirms it.
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
      surface: 'observability_score_event',
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

function toAssayScoreArtifact(score: ExportedScore): AssayScoreArtifact {
  const targetRef = score.spanId ?? score.traceId ?? score.correlationContext?.entityId;
  if (!targetRef) {
    throw new Error('Score event is missing a bounded target anchor');
  }

  return {
    schema: 'mastra.score-event.export.v1',
    framework: 'mastra',
    surface: 'observability_score_event',
    timestamp: score.timestamp.toISOString(),
    scorer_id: score.scorerId,
    scorer_name: score.scorerName ?? score.scorerId,
    score: score.score,
    target_ref: targetRef,
    ...(score.targetEntityType ? { target_entity_type: String(score.targetEntityType).toLowerCase() } : {}),
    ...(score.reason ? { reason: score.reason } : {}),
    ...(score.traceId ? { trace_id_ref: score.traceId } : {}),
    ...(score.spanId ? { span_id_ref: score.spanId } : {}),
    ...(score.scorerVersion ? { scorer_version: score.scorerVersion } : {}),
    ...(score.scoreSource ? { score_source: score.scoreSource } : {}),
    ...(score.metadata ? { metadata_ref: 'metadata:redacted' } : {}),
  };
}
