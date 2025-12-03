/**
 * Usage Example for ReplayResultDialog Component
 *
 * This file demonstrates how to integrate the ReplayResultDialog
 * into your chat interface for PRD-02 Deterministic Replay.
 *
 * Copyright JKCA | 2025 James KC Auchterlonie
 */

import React, { useState } from 'react';
import { Button } from '@/components/ui/button';
import { ReplayResultDialog } from './ReplayResultDialog';
import type { ReplayResponse } from '@/api/replay-types';

/**
 * Example 1: Basic Usage
 *
 * Shows how to display replay results after executing a replay.
 */
export function BasicReplayResultExample() {
  const [dialogOpen, setDialogOpen] = useState(false);
  const [replayResult, setReplayResult] = useState<ReplayResponse | null>(null);

  const handleReplayClick = async () => {
    // Execute replay via API
    const response = await fetch('/api/v1/replay', {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({
        inference_id: 'inference-123',
        allow_approximate: true,
      }),
    });

    const result: ReplayResponse = await response.json();
    setReplayResult(result);
    setDialogOpen(true);
  };

  return (
    <>
      <Button onClick={handleReplayClick}>
        Replay Inference
      </Button>

      <ReplayResultDialog
        open={dialogOpen}
        onOpenChange={setDialogOpen}
        replayResponse={replayResult}
      />
    </>
  );
}

/**
 * Example 2: With History Integration
 *
 * Shows how to add a "View History" button that navigates
 * to the replay history page.
 */
export function ReplayResultWithHistoryExample() {
  const [dialogOpen, setDialogOpen] = useState(false);
  const [replayResult, setReplayResult] = useState<ReplayResponse | null>(null);

  const handleViewHistory = () => {
    // Navigate to replay history page
    window.location.href = `/replay/history/${replayResult?.original_inference_id}`;
  };

  return (
    <ReplayResultDialog
      open={dialogOpen}
      onOpenChange={setDialogOpen}
      replayResponse={replayResult}
      onViewHistory={handleViewHistory}
    />
  );
}

/**
 * Example 3: Chat Message Integration
 *
 * Shows how to add a replay button to chat messages
 * and display results in the dialog.
 */
export function ChatMessageWithReplayExample() {
  const [dialogOpen, setDialogOpen] = useState(false);
  const [replayResult, setReplayResult] = useState<ReplayResponse | null>(null);
  const [isReplaying, setIsReplaying] = useState(false);

  const handleReplayMessage = async (inferenceId: string) => {
    setIsReplaying(true);
    try {
      const response = await fetch('/api/v1/replay', {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({
          inference_id: inferenceId,
          allow_approximate: true,
        }),
      });

      if (!response.ok) {
        throw new Error('Replay failed');
      }

      const result: ReplayResponse = await response.json();
      setReplayResult(result);
      setDialogOpen(true);
    } catch (error) {
      console.error('Replay error:', error);
      // Handle error (show toast, etc.)
    } finally {
      setIsReplaying(false);
    }
  };

  return (
    <div className="chat-message">
      <div className="message-content">
        Hello, this is a chat message response.
      </div>
      <div className="message-actions">
        <Button
          variant="ghost"
          size="sm"
          onClick={() => handleReplayMessage('inference-123')}
          disabled={isReplaying}
        >
          {isReplaying ? 'Replaying...' : 'Replay'}
        </Button>
      </div>

      <ReplayResultDialog
        open={dialogOpen}
        onOpenChange={setDialogOpen}
        replayResponse={replayResult}
      />
    </div>
  );
}

/**
 * Example 4: Mock Data for Testing
 *
 * Useful for testing the dialog UI without backend integration.
 */
export function ReplayResultMockExample() {
  const [dialogOpen, setDialogOpen] = useState(false);

  const mockReplayResponse: ReplayResponse = {
    replay_id: 'replay-456',
    original_inference_id: 'inference-123',
    replay_mode: 'exact',
    response: 'This is the replayed response text. It matches the original exactly!',
    response_truncated: false,
    match_status: 'exact',
    original_response: 'This is the replayed response text. It matches the original exactly!',
    stats: {
      tokens_generated: 42,
      latency_ms: 156,
      original_latency_ms: 150,
    },
  };

  const mockDivergentResponse: ReplayResponse = {
    replay_id: 'replay-789',
    original_inference_id: 'inference-456',
    replay_mode: 'approximate',
    response: 'This is a slightly different response due to approximation.',
    response_truncated: false,
    match_status: 'semantic',
    divergence: {
      divergence_position: 10,
      backend_changed: true,
      manifest_changed: false,
      approximation_reasons: [
        'Backend changed from CoreML to MLX',
        'RAG documents partially unavailable',
      ],
    },
    rag_reproducibility: {
      score: 0.75,
      matching_docs: 3,
      total_original_docs: 4,
      missing_doc_ids: ['doc-missing-1'],
    },
    original_response: 'This is the original response text before replay.',
    stats: {
      tokens_generated: 38,
      latency_ms: 175,
      original_latency_ms: 160,
    },
  };

  return (
    <div className="space-y-4">
      <Button onClick={() => setDialogOpen(true)}>
        Show Exact Match Result
      </Button>

      <ReplayResultDialog
        open={dialogOpen}
        onOpenChange={setDialogOpen}
        replayResponse={mockReplayResponse}
      />
    </div>
  );
}

/**
 * Example 5: Replay Availability Check
 *
 * Shows how to check if replay is available before showing
 * the replay button.
 */
export function ReplayAvailabilityExample() {
  const [canReplay, setCanReplay] = useState(false);
  const [dialogOpen, setDialogOpen] = useState(false);
  const [replayResult, setReplayResult] = useState<ReplayResponse | null>(null);

  React.useEffect(() => {
    // Check replay availability
    fetch('/api/v1/replay/availability/inference-123')
      .then((res) => res.json())
      .then((data) => {
        setCanReplay(data.can_replay_exact || data.can_replay_approximate);
      });
  }, []);

  const handleReplay = async () => {
    const response = await fetch('/api/v1/replay', {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({
        inference_id: 'inference-123',
        allow_approximate: true,
      }),
    });

    const result: ReplayResponse = await response.json();
    setReplayResult(result);
    setDialogOpen(true);
  };

  return (
    <>
      {canReplay && (
        <Button onClick={handleReplay}>
          Replay
        </Button>
      )}

      <ReplayResultDialog
        open={dialogOpen}
        onOpenChange={setDialogOpen}
        replayResponse={replayResult}
      />
    </>
  );
}
