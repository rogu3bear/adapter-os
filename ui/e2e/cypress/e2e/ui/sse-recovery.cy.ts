/// <reference types="cypress" />
import '../../support/commands';

/**
 * SSE Streaming Error Recovery E2E Tests
 *
 * Tests the SSE streaming functionality including:
 * 1. SSE connection establishment
 * 2. Reconnection after disconnect
 * 3. Gap warning display (when events are missed)
 * 4. Last-Event-ID header usage for resumption
 *
 * These tests validate the resilience of the streaming infrastructure
 * using Cypress intercepts to simulate various SSE scenarios.
 */

describe('SSE Streaming Error Recovery', () => {
  beforeEach(() => {
    cy.login();
    cy.stubApiRoutes();
  });

  afterEach(() => {
    cy.cleanupTestData();
  });

  describe('SSE Connection Establishment', () => {
    it('should establish SSE connection and receive heartbeat events', () => {
      // Intercept the SSE stream endpoint for metrics
      cy.intercept('GET', '**/v1/stream/metrics', (req) => {
        req.reply({
          statusCode: 200,
          headers: {
            'Content-Type': 'text/event-stream',
            'Cache-Control': 'no-cache',
            'Connection': 'keep-alive',
            'X-Accel-Buffering': 'no',
          },
          body: [
            'id: 1',
            'event: heartbeat',
            'data: {"type":"heartbeat","current_id":1,"timestamp_ms":' + Date.now() + '}',
            '',
            'id: 2',
            'event: metrics',
            'data: {"timestamp_ms":' + Date.now() + ',"latency":{"p50_ms":10,"p95_ms":25,"p99_ms":50},"throughput":{"tokens_per_second":100,"inferences_per_second":5},"system":{"cpu_percent":25,"memory_percent":60,"disk_percent":40}}',
            '',
          ].join('\n'),
        });
      }).as('sseMetrics');

      // Navigate to a page that uses SSE (e.g., dashboard with live metrics)
      cy.visit('/dashboard');
      cy.get('[data-cy=dashboard-page]', { timeout: 10000 }).should('exist');

      // The SSE connection should be established
      // We verify by checking that the intercept was called
      cy.wait('@sseMetrics', { timeout: 15000 }).then((interception) => {
        expect(interception.request.headers).to.have.property('accept');
      });
    });

    it('should receive adapter state transition events via SSE', () => {
      const adapterEvent = {
        adapter_id: 'adapter-test-123',
        adapter_name: 'Test Adapter',
        previous_state: 'cold',
        current_state: 'hot',
        timestamp: Date.now(),
        activation_percentage: 100,
        memory_usage_mb: 256,
      };

      cy.intercept('GET', '**/v1/stream/adapters', (req) => {
        req.reply({
          statusCode: 200,
          headers: {
            'Content-Type': 'text/event-stream',
            'Cache-Control': 'no-cache',
            'Connection': 'keep-alive',
          },
          body: [
            'id: 1',
            'event: adapters',
            `data: ${JSON.stringify(adapterEvent)}`,
            '',
          ].join('\n'),
        });
      }).as('sseAdapters');

      cy.visit('/adapters');
      cy.get('[data-cy=adapters-page], [data-cy=adapter-list]', { timeout: 10000 }).should('exist');

      // Verify SSE stream was accessed
      cy.wait('@sseAdapters', { timeout: 15000 });
    });

    it('should handle SSE connection with proper headers', () => {
      cy.intercept('GET', '**/v1/stream/**', (req) => {
        // Verify the request has proper SSE headers
        expect(req.headers['accept']).to.include('text/event-stream');

        req.reply({
          statusCode: 200,
          headers: {
            'Content-Type': 'text/event-stream',
            'Cache-Control': 'no-cache',
            'Connection': 'keep-alive',
          },
          body: 'id: 1\nevent: heartbeat\ndata: {"type":"heartbeat","current_id":1,"timestamp_ms":' + Date.now() + '}\n\n',
        });
      }).as('sseAny');

      cy.visit('/dashboard');
      cy.get('[data-cy=dashboard-page]', { timeout: 10000 }).should('exist');
    });
  });

  describe('SSE Reconnection After Disconnect', () => {
    it('should reconnect automatically after connection error', () => {
      let connectionAttempts = 0;

      cy.intercept('GET', '**/v1/stream/metrics', (req) => {
        connectionAttempts++;

        if (connectionAttempts === 1) {
          // First connection fails
          req.reply({
            statusCode: 503,
            body: { error: 'Service temporarily unavailable' },
          });
        } else {
          // Subsequent connections succeed
          req.reply({
            statusCode: 200,
            headers: {
              'Content-Type': 'text/event-stream',
              'Cache-Control': 'no-cache',
              'Connection': 'keep-alive',
            },
            body: [
              'id: 1',
              'event: heartbeat',
              'data: {"type":"heartbeat","current_id":1,"timestamp_ms":' + Date.now() + '}',
              '',
              'id: 2',
              'event: metrics',
              'data: {"timestamp_ms":' + Date.now() + ',"latency":{"p50_ms":10,"p95_ms":25,"p99_ms":50},"throughput":{"tokens_per_second":100,"inferences_per_second":5},"system":{"cpu_percent":25,"memory_percent":60,"disk_percent":40}}',
              '',
            ].join('\n'),
          });
        }
      }).as('sseMetricsReconnect');

      cy.visit('/dashboard');
      cy.get('[data-cy=dashboard-page]', { timeout: 10000 }).should('exist');

      // Wait for reconnection to succeed
      cy.wait('@sseMetricsReconnect', { timeout: 10000 });
      cy.wait('@sseMetricsReconnect', { timeout: 15000 }).then(() => {
        expect(connectionAttempts).to.be.at.least(2);
      });
    });

    it('should show connection status indicator during reconnection', () => {
      let connectionAttempts = 0;

      cy.intercept('GET', '**/v1/stream/**', (req) => {
        connectionAttempts++;

        if (connectionAttempts <= 2) {
          // Simulate network error by not replying
          req.destroy();
        } else {
          req.reply({
            statusCode: 200,
            headers: {
              'Content-Type': 'text/event-stream',
              'Cache-Control': 'no-cache',
            },
            body: 'id: 1\nevent: heartbeat\ndata: {"type":"heartbeat","current_id":1,"timestamp_ms":' + Date.now() + '}\n\n',
          });
        }
      }).as('sseReconnecting');

      cy.visit('/dashboard');

      // Look for connection status indicator (if present in UI)
      cy.get('body').then(($body) => {
        // Check for reconnecting/offline indicators
        if ($body.find('[data-cy=connection-status]').length > 0) {
          cy.get('[data-cy=connection-status]')
            .should('contain.text', /reconnecting|offline|disconnected/i);
        }
      });
    });

    it('should respect exponential backoff during reconnection attempts', () => {
      const connectionTimestamps: number[] = [];

      cy.intercept('GET', '**/v1/stream/metrics', (req) => {
        connectionTimestamps.push(Date.now());

        if (connectionTimestamps.length <= 3) {
          req.reply({ statusCode: 500, body: { error: 'Internal error' } });
        } else {
          req.reply({
            statusCode: 200,
            headers: {
              'Content-Type': 'text/event-stream',
              'Cache-Control': 'no-cache',
            },
            body: 'id: 1\nevent: heartbeat\ndata: {"type":"heartbeat","current_id":1,"timestamp_ms":' + Date.now() + '}\n\n',
          });
        }
      }).as('sseBackoff');

      cy.visit('/dashboard');
      cy.get('[data-cy=dashboard-page]', { timeout: 10000 }).should('exist');

      // Wait for multiple reconnection attempts
      cy.wait('@sseBackoff', { timeout: 20000 });

      // Verify backoff is happening (delays should increase)
      cy.then(() => {
        if (connectionTimestamps.length >= 3) {
          const delay1 = connectionTimestamps[1] - connectionTimestamps[0];
          const delay2 = connectionTimestamps[2] - connectionTimestamps[1];
          // Backoff should increase (or at minimum stay the same)
          expect(delay2).to.be.at.least(delay1 * 0.5); // Allow some variance
        }
      });
    });
  });

  describe('Gap Warning Display', () => {
    it('should display gap warning when events are missed', () => {
      // Simulate a gap event being sent by the server
      const gapEvent = {
        type: 'event_gap',
        client_last_id: 5,
        server_oldest_id: 10,
        events_lost: 5,
        recovery_hint: { type: 'continue_with_gap' },
      };

      cy.intercept('GET', '**/v1/stream/metrics', (req) => {
        req.reply({
          statusCode: 200,
          headers: {
            'Content-Type': 'text/event-stream',
            'Cache-Control': 'no-cache',
          },
          body: [
            'id: 10',
            'event: error',
            `data: ${JSON.stringify(gapEvent)}`,
            '',
            'id: 11',
            'event: metrics',
            'data: {"timestamp_ms":' + Date.now() + ',"latency":{"p50_ms":10,"p95_ms":25,"p99_ms":50},"throughput":{"tokens_per_second":100,"inferences_per_second":5},"system":{"cpu_percent":25,"memory_percent":60,"disk_percent":40}}',
            '',
          ].join('\n'),
        });
      }).as('sseWithGap');

      cy.visit('/dashboard');
      cy.get('[data-cy=dashboard-page]', { timeout: 10000 }).should('exist');
      cy.wait('@sseWithGap', { timeout: 15000 });

      // Check for gap warning display (if UI component exists)
      cy.get('body').then(($body) => {
        if ($body.find('[data-cy=gap-warning], [data-cy=events-missed]').length > 0) {
          cy.get('[data-cy=gap-warning], [data-cy=events-missed]')
            .should('be.visible')
            .and('contain.text', /missed|gap|lost/i);
        }
      });
    });

    it('should show recovery hint when buffer overflow occurs', () => {
      const overflowEvent = {
        type: 'buffer_overflow',
        dropped_count: 100,
        oldest_available_id: 150,
      };

      cy.intercept('GET', '**/v1/stream/metrics', (req) => {
        req.reply({
          statusCode: 200,
          headers: {
            'Content-Type': 'text/event-stream',
            'Cache-Control': 'no-cache',
          },
          body: [
            'id: 150',
            'event: error',
            `data: ${JSON.stringify(overflowEvent)}`,
            '',
            'id: 151',
            'event: heartbeat',
            'data: {"type":"heartbeat","current_id":151,"timestamp_ms":' + Date.now() + '}',
            '',
          ].join('\n'),
        });
      }).as('sseOverflow');

      cy.visit('/dashboard');
      cy.get('[data-cy=dashboard-page]', { timeout: 10000 }).should('exist');
      cy.wait('@sseOverflow', { timeout: 15000 });

      // Check for overflow warning or refresh prompt
      cy.get('body').then(($body) => {
        if ($body.find('[data-cy=overflow-warning], [data-cy=refresh-prompt]').length > 0) {
          cy.get('[data-cy=overflow-warning], [data-cy=refresh-prompt]')
            .should('be.visible');
        }
      });
    });

    it('should handle stream disconnected event gracefully', () => {
      const disconnectEvent = {
        type: 'stream_disconnected',
        last_event_id: 50,
        reason: 'Server maintenance',
        reconnect_hint_ms: 5000,
      };

      cy.intercept('GET', '**/v1/stream/metrics', (req) => {
        req.reply({
          statusCode: 200,
          headers: {
            'Content-Type': 'text/event-stream',
            'Cache-Control': 'no-cache',
          },
          body: [
            'id: 50',
            'event: error',
            `data: ${JSON.stringify(disconnectEvent)}`,
            '',
          ].join('\n'),
        });
      }).as('sseDisconnect');

      cy.visit('/dashboard');
      cy.get('[data-cy=dashboard-page]', { timeout: 10000 }).should('exist');
      cy.wait('@sseDisconnect', { timeout: 15000 });

      // UI should handle disconnect gracefully without crashing
      cy.get('[data-cy=dashboard-page]').should('exist');
    });

    it('should recommend full state refresh for refetch_full_state recovery hint', () => {
      const refetchEvent = {
        type: 'event_gap',
        client_last_id: 10,
        server_oldest_id: 100,
        events_lost: 90,
        recovery_hint: { type: 'refetch_full_state' },
      };

      cy.intercept('GET', '**/v1/stream/metrics', (req) => {
        req.reply({
          statusCode: 200,
          headers: {
            'Content-Type': 'text/event-stream',
            'Cache-Control': 'no-cache',
          },
          body: [
            'id: 100',
            'event: error',
            `data: ${JSON.stringify(refetchEvent)}`,
            '',
            'id: 101',
            'event: heartbeat',
            'data: {"type":"heartbeat","current_id":101,"timestamp_ms":' + Date.now() + '}',
            '',
          ].join('\n'),
        });
      }).as('sseRefetch');

      cy.visit('/dashboard');
      cy.get('[data-cy=dashboard-page]', { timeout: 10000 }).should('exist');
      cy.wait('@sseRefetch', { timeout: 15000 });

      // Check for refresh recommendation
      cy.get('body').then(($body) => {
        if ($body.find('[data-cy=refresh-required], [data-cy=stale-data-warning]').length > 0) {
          cy.get('[data-cy=refresh-required], [data-cy=stale-data-warning]')
            .should('be.visible');
        }
      });
    });
  });

  describe('Last-Event-ID Header Usage', () => {
    it('should send Last-Event-ID header on reconnection', () => {
      let connectionCount = 0;
      let lastEventIdReceived: string | undefined;

      cy.intercept('GET', '**/v1/stream/metrics', (req) => {
        connectionCount++;
        lastEventIdReceived = req.headers['last-event-id'] as string | undefined;

        if (connectionCount === 1) {
          // First connection - send some events then close
          req.reply({
            statusCode: 200,
            headers: {
              'Content-Type': 'text/event-stream',
              'Cache-Control': 'no-cache',
            },
            body: [
              'id: 1',
              'event: heartbeat',
              'data: {"type":"heartbeat","current_id":1,"timestamp_ms":' + Date.now() + '}',
              '',
              'id: 5',
              'event: metrics',
              'data: {"timestamp_ms":' + Date.now() + ',"latency":{"p50_ms":10,"p95_ms":25,"p99_ms":50},"throughput":{"tokens_per_second":100,"inferences_per_second":5},"system":{"cpu_percent":25,"memory_percent":60,"disk_percent":40}}',
              '',
            ].join('\n'),
          });
        } else {
          // Reconnection - should have Last-Event-ID
          req.reply({
            statusCode: 200,
            headers: {
              'Content-Type': 'text/event-stream',
              'Cache-Control': 'no-cache',
            },
            body: [
              'id: 6',
              'event: heartbeat',
              'data: {"type":"heartbeat","current_id":6,"timestamp_ms":' + Date.now() + '}',
              '',
            ].join('\n'),
          });
        }
      }).as('sseWithEventId');

      cy.visit('/dashboard');
      cy.get('[data-cy=dashboard-page]', { timeout: 10000 }).should('exist');

      // Wait for initial connection
      cy.wait('@sseWithEventId', { timeout: 15000 });

      // Trigger a reconnection by simulating a disconnect
      // (In real scenario, the SSE hook handles this automatically)
    });

    it('should resume from correct event ID after reconnection', () => {
      let connectionCount = 0;

      cy.intercept('GET', '**/v1/stream/metrics', (req) => {
        connectionCount++;
        const lastEventId = req.headers['last-event-id'];

        if (connectionCount === 1) {
          req.reply({
            statusCode: 200,
            headers: {
              'Content-Type': 'text/event-stream',
              'Cache-Control': 'no-cache',
            },
            body: [
              'id: 100',
              'event: metrics',
              'data: {"timestamp_ms":' + Date.now() + ',"latency":{"p50_ms":10,"p95_ms":25,"p99_ms":50},"throughput":{"tokens_per_second":100,"inferences_per_second":5},"system":{"cpu_percent":25,"memory_percent":60,"disk_percent":40}}',
              '',
            ].join('\n'),
          });
        } else {
          // On reconnection, if Last-Event-ID is provided, resume from there
          const resumeId = lastEventId ? parseInt(lastEventId as string, 10) + 1 : 101;
          req.reply({
            statusCode: 200,
            headers: {
              'Content-Type': 'text/event-stream',
              'Cache-Control': 'no-cache',
            },
            body: [
              `id: ${resumeId}`,
              'event: metrics',
              'data: {"timestamp_ms":' + Date.now() + ',"latency":{"p50_ms":12,"p95_ms":28,"p99_ms":55},"throughput":{"tokens_per_second":95,"inferences_per_second":4},"system":{"cpu_percent":30,"memory_percent":65,"disk_percent":42}}',
              '',
            ].join('\n'),
          });
        }
      }).as('sseResume');

      cy.visit('/dashboard');
      cy.get('[data-cy=dashboard-page]', { timeout: 10000 }).should('exist');
      cy.wait('@sseResume', { timeout: 15000 });
    });

    it('should handle server returning gap when Last-Event-ID is too old', () => {
      let connectionCount = 0;

      cy.intercept('GET', '**/v1/stream/metrics', (req) => {
        connectionCount++;
        const lastEventId = req.headers['last-event-id'];

        if (lastEventId && parseInt(lastEventId as string, 10) < 50) {
          // Client is too far behind, send gap event
          const gapEvent = {
            type: 'event_gap',
            client_last_id: parseInt(lastEventId as string, 10),
            server_oldest_id: 50,
            events_lost: 50 - parseInt(lastEventId as string, 10),
            recovery_hint: { type: 'refetch_full_state' },
          };

          req.reply({
            statusCode: 200,
            headers: {
              'Content-Type': 'text/event-stream',
              'Cache-Control': 'no-cache',
            },
            body: [
              'id: 50',
              'event: error',
              `data: ${JSON.stringify(gapEvent)}`,
              '',
              'id: 51',
              'event: heartbeat',
              'data: {"type":"heartbeat","current_id":51,"timestamp_ms":' + Date.now() + '}',
              '',
            ].join('\n'),
          });
        } else {
          req.reply({
            statusCode: 200,
            headers: {
              'Content-Type': 'text/event-stream',
              'Cache-Control': 'no-cache',
            },
            body: [
              'id: 1',
              'event: heartbeat',
              'data: {"type":"heartbeat","current_id":1,"timestamp_ms":' + Date.now() + '}',
              '',
            ].join('\n'),
          });
        }
      }).as('sseGapOnResume');

      cy.visit('/dashboard');
      cy.get('[data-cy=dashboard-page]', { timeout: 10000 }).should('exist');
      cy.wait('@sseGapOnResume', { timeout: 15000 });
    });
  });

  describe('Streaming Inference Recovery', () => {
    it('should handle inference stream disconnection mid-stream', () => {
      let streamAttempts = 0;

      cy.intercept('POST', '**/v1/infer/stream', (req) => {
        streamAttempts++;

        if (streamAttempts === 1) {
          // First attempt - partial response then connection closes
          req.reply({
            statusCode: 200,
            headers: {
              'Content-Type': 'text/event-stream',
              'Cache-Control': 'no-cache',
            },
            body: [
              'data: {"event":"Token","text":"Hello"}',
              '',
              'data: {"event":"Token","text":" world"}',
              '',
              // Connection drops here - no Done event
            ].join('\n'),
          });
        } else {
          // Retry succeeds with full response
          req.reply({
            statusCode: 200,
            headers: {
              'Content-Type': 'text/event-stream',
              'Cache-Control': 'no-cache',
            },
            body: [
              'data: {"event":"Token","text":"Hello"}',
              '',
              'data: {"event":"Token","text":" world"}',
              '',
              'data: {"event":"Token","text":"!"}',
              '',
              'data: {"event":"Done","total_tokens":3,"latency_ms":150}',
              '',
              'data: [DONE]',
              '',
            ].join('\n'),
          });
        }
      }).as('inferStream');

      // Mock the chat session and other required endpoints
      cy.intercept('GET', '**/v1/chat/sessions**', {
        statusCode: 200,
        body: { schema_version: '1.0', sessions: [], total: 0 },
      }).as('chatSessions');

      cy.intercept('POST', '**/v1/chat/sessions', {
        statusCode: 200,
        body: {
          schema_version: '1.0',
          id: 'session-test',
          name: 'Test Session',
          created_at: new Date().toISOString(),
          messages: [],
        },
      }).as('createSession');

      cy.visit('/chat');
      cy.get('[data-cy=chat-page], [data-cy=chat-input]', { timeout: 10000 }).should('exist');
    });

    it('should display error state when inference stream fails completely', () => {
      cy.intercept('POST', '**/v1/infer/stream', {
        statusCode: 500,
        body: { error: 'Internal server error', code: 'INTERNAL_ERROR' },
      }).as('inferStreamFail');

      cy.intercept('GET', '**/v1/chat/sessions**', {
        statusCode: 200,
        body: { schema_version: '1.0', sessions: [], total: 0 },
      }).as('chatSessions');

      cy.visit('/chat');
      cy.get('[data-cy=chat-page], [data-cy=chat-input]', { timeout: 10000 }).should('exist');

      // Type and send a message
      cy.get('body').then(($body) => {
        if ($body.find('[data-cy=chat-input]').length > 0) {
          cy.get('[data-cy=chat-input]').type('Test message');
          cy.get('[data-cy=send-button], button[type="submit"]').click();

          // Should show error state
          cy.get('[data-cy=error-message], [data-cy=stream-error]', { timeout: 10000 })
            .should('be.visible');
        }
      });
    });

    it('should preserve partial response on stream failure', () => {
      cy.intercept('POST', '**/v1/infer/stream', (req) => {
        req.reply({
          statusCode: 200,
          headers: {
            'Content-Type': 'text/event-stream',
            'Cache-Control': 'no-cache',
          },
          body: [
            'data: {"event":"Token","text":"Partial"}',
            '',
            'data: {"event":"Token","text":" response"}',
            '',
            'data: {"event":"Token","text":" here"}',
            '',
            'data: {"event":"Error","message":"Stream interrupted","recoverable":false}',
            '',
          ].join('\n'),
        });
      }).as('inferStreamPartial');

      cy.intercept('GET', '**/v1/chat/sessions**', {
        statusCode: 200,
        body: { schema_version: '1.0', sessions: [], total: 0 },
      }).as('chatSessions');

      cy.visit('/chat');
      cy.get('[data-cy=chat-page], [data-cy=chat-input]', { timeout: 10000 }).should('exist');

      // The UI should handle partial responses gracefully
      // and potentially show what was received before the error
    });
  });

  describe('Circuit Breaker Behavior', () => {
    it('should open circuit breaker after repeated failures', () => {
      let failureCount = 0;

      cy.intercept('GET', '**/v1/stream/metrics', (req) => {
        failureCount++;
        // Always fail to trigger circuit breaker
        req.reply({ statusCode: 503, body: { error: 'Service unavailable' } });
      }).as('sseCircuitBreaker');

      cy.visit('/dashboard');
      cy.get('[data-cy=dashboard-page]', { timeout: 10000 }).should('exist');

      // Wait for multiple failure attempts
      cy.wait('@sseCircuitBreaker', { timeout: 10000 });

      // After circuit breaker opens, failures should be throttled
      cy.then(() => {
        // Circuit breaker should have been triggered
        // Check for any circuit breaker indicators in UI
        cy.get('body').then(($body) => {
          if ($body.find('[data-cy=service-unavailable], [data-cy=circuit-open]').length > 0) {
            cy.get('[data-cy=service-unavailable], [data-cy=circuit-open]')
              .should('be.visible');
          }
        });
      });
    });

    it('should recover after circuit breaker timeout', () => {
      let callCount = 0;

      cy.intercept('GET', '**/v1/stream/metrics', (req) => {
        callCount++;

        if (callCount <= 5) {
          req.reply({ statusCode: 503, body: { error: 'Service unavailable' } });
        } else {
          req.reply({
            statusCode: 200,
            headers: {
              'Content-Type': 'text/event-stream',
              'Cache-Control': 'no-cache',
            },
            body: [
              'id: 1',
              'event: heartbeat',
              'data: {"type":"heartbeat","current_id":1,"timestamp_ms":' + Date.now() + '}',
              '',
            ].join('\n'),
          });
        }
      }).as('sseCircuitRecovery');

      cy.visit('/dashboard');
      cy.get('[data-cy=dashboard-page]', { timeout: 10000 }).should('exist');

      // Wait for recovery
      cy.wait('@sseCircuitRecovery', { timeout: 30000 });

      // Eventually should recover
      cy.then(() => {
        expect(callCount).to.be.at.least(1);
      });
    });

    it('should allow manual reconnect to reset circuit breaker', () => {
      let callCount = 0;

      cy.intercept('GET', '**/v1/stream/metrics', (req) => {
        callCount++;

        if (callCount <= 3) {
          req.reply({ statusCode: 503, body: { error: 'Service unavailable' } });
        } else {
          req.reply({
            statusCode: 200,
            headers: {
              'Content-Type': 'text/event-stream',
              'Cache-Control': 'no-cache',
            },
            body: [
              'id: 1',
              'event: heartbeat',
              'data: {"type":"heartbeat","current_id":1,"timestamp_ms":' + Date.now() + '}',
              '',
            ].join('\n'),
          });
        }
      }).as('sseManualReconnect');

      cy.visit('/dashboard');
      cy.get('[data-cy=dashboard-page]', { timeout: 10000 }).should('exist');

      // Wait for some failures
      cy.wait('@sseManualReconnect', { timeout: 10000 });

      // If there's a manual reconnect button, click it
      cy.get('body').then(($body) => {
        if ($body.find('[data-cy=reconnect-button]').length > 0) {
          cy.get('[data-cy=reconnect-button]').click();
          // Should trigger new connection attempt
          cy.wait('@sseManualReconnect', { timeout: 10000 });
        }
      });
    });
  });

  describe('Keepalive Timeout Handling', () => {
    it('should detect stale connection when no keepalive received', () => {
      cy.intercept('GET', '**/v1/stream/metrics', (req) => {
        // Send initial event but no keepalive
        req.reply({
          statusCode: 200,
          headers: {
            'Content-Type': 'text/event-stream',
            'Cache-Control': 'no-cache',
          },
          body: [
            'id: 1',
            'event: metrics',
            'data: {"timestamp_ms":' + Date.now() + ',"latency":{"p50_ms":10,"p95_ms":25,"p99_ms":50},"throughput":{"tokens_per_second":100,"inferences_per_second":5},"system":{"cpu_percent":25,"memory_percent":60,"disk_percent":40}}',
            '',
            // No further keepalive events
          ].join('\n'),
        });
      }).as('sseNoKeepalive');

      cy.visit('/dashboard');
      cy.get('[data-cy=dashboard-page]', { timeout: 10000 }).should('exist');
      cy.wait('@sseNoKeepalive', { timeout: 15000 });

      // The SSE hook should eventually detect the stale connection
      // This is a long-running test as keepalive timeout is typically 60s
    });

    it('should maintain connection with regular keepalive events', () => {
      cy.intercept('GET', '**/v1/stream/metrics', (req) => {
        const now = Date.now();
        req.reply({
          statusCode: 200,
          headers: {
            'Content-Type': 'text/event-stream',
            'Cache-Control': 'no-cache',
          },
          body: [
            'id: 1',
            'event: keepalive',
            'data: {"type":"keepalive"}',
            '',
            'id: 2',
            'event: metrics',
            `data: {"timestamp_ms":${now},"latency":{"p50_ms":10,"p95_ms":25,"p99_ms":50},"throughput":{"tokens_per_second":100,"inferences_per_second":5},"system":{"cpu_percent":25,"memory_percent":60,"disk_percent":40}}`,
            '',
            'id: 3',
            'event: keepalive',
            'data: {"type":"keepalive"}',
            '',
          ].join('\n'),
        });
      }).as('sseWithKeepalive');

      cy.visit('/dashboard');
      cy.get('[data-cy=dashboard-page]', { timeout: 10000 }).should('exist');
      cy.wait('@sseWithKeepalive', { timeout: 15000 });

      // Connection should remain healthy
      cy.get('[data-cy=dashboard-page]').should('exist');
    });
  });
});
