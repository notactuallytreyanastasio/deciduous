/**
 * ChatPanel Component
 *
 * Chat interface for asking questions about a selected narrative.
 * Connects to /api/ask to shell out to Claude.
 */

import React, { useState, useRef, useEffect, useCallback } from 'react';
import ReactMarkdown from 'react-markdown';
import type { Narrative, ChatMessage, NarrativeContext, ArchaeologyAskContext } from '../types/archaeology';
import { formatNarrativeContext } from '../utils/archaeologyProcessing';
import { getNodeColor } from '../utils/colors';
import { useLocalStorage } from '../hooks/useLocalStorage';

interface ChatPanelProps {
  narrative: Narrative | null;
  onSelectNode?: (nodeId: number) => void;
}

// Generate unique ID for messages
const generateId = () => Math.random().toString(36).substring(2, 9);

// Request timeout in milliseconds
const REQUEST_TIMEOUT_MS = 60000;

// Serializable version of ChatMessage for localStorage
interface StoredMessage {
  id: string;
  role: 'user' | 'assistant';
  content: string;
  timestamp: string; // ISO string
}

function toStoredMessages(messages: ChatMessage[]): StoredMessage[] {
  return messages.map(m => ({
    ...m,
    timestamp: m.timestamp.toISOString(),
  }));
}

function fromStoredMessages(stored: StoredMessage[]): ChatMessage[] {
  return stored.map(m => ({
    ...m,
    timestamp: new Date(m.timestamp),
  }));
}

export const ChatPanel: React.FC<ChatPanelProps> = ({
  narrative,
}) => {
  // Persist chat history per narrative
  const [chatHistoryMap, setChatHistoryMap] = useLocalStorage<Record<string, StoredMessage[]>>(
    'chat_history',
    {}
  );

  // Get/set messages for current narrative
  const narrativeKey = narrative?.id ?? '';
  const messages = narrativeKey && chatHistoryMap[narrativeKey]
    ? fromStoredMessages(chatHistoryMap[narrativeKey])
    : [];

  const setMessages = useCallback((updater: ChatMessage[] | ((prev: ChatMessage[]) => ChatMessage[])) => {
    if (!narrativeKey) return;
    setChatHistoryMap(prev => {
      const currentMessages = prev[narrativeKey]
        ? fromStoredMessages(prev[narrativeKey])
        : [];
      const newMessages = typeof updater === 'function' ? updater(currentMessages) : updater;
      return {
        ...prev,
        [narrativeKey]: toStoredMessages(newMessages),
      };
    });
  }, [narrativeKey, setChatHistoryMap]);

  const [input, setInput] = useState('');
  const [isLoading, setIsLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [lastFailedMessage, setLastFailedMessage] = useState<ChatMessage | null>(null);
  const messagesEndRef = useRef<HTMLDivElement>(null);
  const abortControllerRef = useRef<AbortController | null>(null);

  // Scroll to bottom when messages change
  useEffect(() => {
    messagesEndRef.current?.scrollIntoView({ behavior: 'smooth' });
  }, [messages]);

  // Clear error state when narrative changes (messages are persisted)
  useEffect(() => {
    setError(null);
    setLastFailedMessage(null);
    // Cancel any pending request
    if (abortControllerRef.current) {
      abortControllerRef.current.abort();
      abortControllerRef.current = null;
    }
  }, [narrative?.id]);

  // Cleanup on unmount
  useEffect(() => {
    return () => {
      if (abortControllerRef.current) {
        abortControllerRef.current.abort();
      }
    };
  }, []);

  // Core send function - can be called with a message directly (for retry)
  const sendMessage = useCallback(async (userMessage: ChatMessage) => {
    if (!narrative) return;

    // Cancel any existing request
    if (abortControllerRef.current) {
      abortControllerRef.current.abort();
    }

    abortControllerRef.current = new AbortController();
    const { signal } = abortControllerRef.current;

    setIsLoading(true);
    setError(null);
    setLastFailedMessage(null);

    try {
      // Build context for the API
      const narrativeContext: NarrativeContext = formatNarrativeContext(narrative);
      const askContext: ArchaeologyAskContext = {
        narrative: narrativeContext,
        visible_node_ids: narrative.nodes.map(n => n.id),
      };

      // Create timeout promise
      const timeoutPromise = new Promise<never>((_, reject) => {
        setTimeout(() => reject(new Error('Request timed out')), REQUEST_TIMEOUT_MS);
      });

      // Race fetch against timeout
      const response = await Promise.race([
        fetch('/api/ask', {
          method: 'POST',
          headers: { 'Content-Type': 'application/json' },
          body: JSON.stringify({
            question: userMessage.content,
            context: askContext,
          }),
          signal,
        }),
        timeoutPromise,
      ]);

      const result = await response.json();

      if (!result.ok) {
        throw new Error(result.error || 'Failed to get response');
      }

      const assistantMessage: ChatMessage = {
        id: generateId(),
        role: 'assistant',
        content: result.data.answer,
        timestamp: new Date(),
      };

      setMessages(prev => [...prev, assistantMessage]);
    } catch (err) {
      if (err instanceof Error && err.name === 'AbortError') {
        // Request was cancelled - don't show error
        return;
      }
      const errorMsg = err instanceof Error ? err.message : 'Unknown error';
      setError(errorMsg);
      setLastFailedMessage(userMessage);
    } finally {
      setIsLoading(false);
      abortControllerRef.current = null;
    }
  }, [narrative]);

  const handleSubmit = async (e: React.FormEvent) => {
    e.preventDefault();
    if (!input.trim() || !narrative || isLoading) return;

    const userMessage: ChatMessage = {
      id: generateId(),
      role: 'user',
      content: input.trim(),
      timestamp: new Date(),
    };

    setMessages(prev => [...prev, userMessage]);
    setInput('');
    await sendMessage(userMessage);
  };

  const handleRetry = useCallback(() => {
    if (lastFailedMessage) {
      sendMessage(lastFailedMessage);
    }
  }, [lastFailedMessage, sendMessage]);

  const handleCancel = useCallback(() => {
    if (abortControllerRef.current) {
      abortControllerRef.current.abort();
      abortControllerRef.current = null;
    }
    setIsLoading(false);
  }, []);

  const handleClearHistory = useCallback(() => {
    if (!narrativeKey) return;
    setChatHistoryMap(prev => {
      const next = { ...prev };
      delete next[narrativeKey];
      return next;
    });
  }, [narrativeKey, setChatHistoryMap]);

  // Render empty state when no narrative selected
  if (!narrative) {
    return (
      <div style={styles.container}>
        <div style={styles.emptyState}>
          <div style={styles.emptyIcon}>?</div>
          <h3 style={styles.emptyTitle}>Select a Narrative</h3>
          <p style={styles.emptyText}>
            Choose a narrative from the list to start asking questions about its history and decisions.
          </p>
        </div>
      </div>
    );
  }

  return (
    <div style={styles.container}>
      {/* Header */}
      <div style={styles.header}>
        <div style={styles.headerTop}>
          <span
            style={{
              ...styles.typeBadge,
              backgroundColor: getNodeColor(narrative.root.node_type) + '22',
              color: getNodeColor(narrative.root.node_type),
            }}
          >
            {narrative.root.node_type}
          </span>
          <span style={styles.nodeCount}>{narrative.nodes.length} nodes</span>
          {messages.length > 0 && (
            <button
              style={styles.clearHistoryButton}
              onClick={handleClearHistory}
              title="Clear chat history"
            >
              Clear
            </button>
          )}
        </div>
        <h2 style={styles.title}>{narrative.name}</h2>
        {narrative.pivots.length > 0 && (
          <div style={styles.pivotBadge}>
            {narrative.pivots.length} pivot{narrative.pivots.length > 1 ? 's' : ''}
          </div>
        )}
      </div>

      {/* GitHub Links */}
      {narrative.githubLinks.length > 0 && (
        <div style={styles.linksSection}>
          <div style={styles.linksHeader}>GitHub Links</div>
          <div style={styles.linksList}>
            {narrative.githubLinks.slice(0, 5).map((link, i) => (
              <a
                key={i}
                href={link.url}
                target="_blank"
                rel="noopener noreferrer"
                style={styles.link}
              >
                {link.type === 'pr' && 'PR #'}
                {link.type === 'issue' && 'Issue #'}
                {link.type === 'commit' ? link.identifier.slice(0, 7) : link.identifier}
              </a>
            ))}
            {narrative.githubLinks.length > 5 && (
              <span style={styles.moreLinks}>
                +{narrative.githubLinks.length - 5} more
              </span>
            )}
          </div>
        </div>
      )}

      {/* Messages */}
      <div style={styles.messages}>
        {messages.length === 0 && (
          <div style={styles.welcomeMessage}>
            <p style={styles.welcomeText}>
              Ask questions about <strong>{narrative.name}</strong>
            </p>
            <div style={styles.suggestions}>
              <button
                style={styles.suggestionButton}
                onClick={() => setInput('What decisions led to the current approach?')}
              >
                What decisions led to the current approach?
              </button>
              {narrative.pivots.length > 0 && (
                <button
                  style={styles.suggestionButton}
                  onClick={() => setInput('Why did we pivot from the original approach?')}
                >
                  Why did we pivot?
                </button>
              )}
              <button
                style={styles.suggestionButton}
                onClick={() => setInput('What were the key observations?')}
              >
                What were the key observations?
              </button>
            </div>
          </div>
        )}

        {messages.map(msg => (
          <div
            key={msg.id}
            style={{
              ...styles.message,
              ...(msg.role === 'user' ? styles.userMessage : styles.assistantMessage),
            }}
          >
            <div style={styles.messageHeader}>
              <span style={styles.messageRole}>
                {msg.role === 'user' ? 'You' : 'Claude'}
              </span>
              <span style={styles.messageTime}>
                {msg.timestamp.toLocaleTimeString()}
              </span>
            </div>
            <div style={styles.messageContent}>
              {msg.role === 'assistant' ? (
                <ReactMarkdown>{msg.content}</ReactMarkdown>
              ) : (
                <p style={{ margin: 0 }}>{msg.content}</p>
              )}
            </div>
          </div>
        ))}

        {isLoading && (
          <div style={{ ...styles.message, ...styles.assistantMessage }}>
            <div style={styles.messageHeader}>
              <span style={styles.messageRole}>Claude</span>
              <button
                style={styles.cancelButton}
                onClick={handleCancel}
                title="Cancel request"
              >
                Cancel
              </button>
            </div>
            <div style={styles.loadingDots}>
              <span className="loading-dot">.</span>
              <span className="loading-dot">.</span>
              <span className="loading-dot">.</span>
            </div>
            <style>{`
              .loading-dot {
                animation: loadingDotPulse 1.4s ease-in-out infinite;
                display: inline-block;
              }
              .loading-dot:nth-child(1) { animation-delay: 0s; }
              .loading-dot:nth-child(2) { animation-delay: 0.2s; }
              .loading-dot:nth-child(3) { animation-delay: 0.4s; }
              @keyframes loadingDotPulse {
                0%, 80%, 100% { opacity: 0.3; transform: scale(1); }
                40% { opacity: 1; transform: scale(1.2); }
              }
            `}</style>
          </div>
        )}

        {error && (
          <div style={styles.errorMessage}>
            <div style={styles.errorContent}>
              <strong>Error:</strong> {error}
            </div>
            {lastFailedMessage && (
              <button
                style={styles.retryButton}
                onClick={handleRetry}
              >
                Retry
              </button>
            )}
          </div>
        )}

        <div ref={messagesEndRef} />
      </div>

      {/* Input */}
      <form onSubmit={handleSubmit} style={styles.inputForm}>
        <input
          type="text"
          value={input}
          onChange={e => setInput(e.target.value)}
          placeholder="Ask about this narrative..."
          style={styles.input}
          disabled={isLoading}
        />
        <button
          type="submit"
          style={{
            ...styles.sendButton,
            opacity: isLoading || !input.trim() ? 0.5 : 1,
          }}
          disabled={isLoading || !input.trim()}
        >
          {isLoading ? '...' : 'Send'}
        </button>
      </form>
    </div>
  );
};

const styles: Record<string, React.CSSProperties> = {
  container: {
    display: 'flex',
    flexDirection: 'column',
    height: '100%',
    backgroundColor: '#ffffff',
    borderLeft: '1px solid #d0d7de',
  },
  emptyState: {
    flex: 1,
    display: 'flex',
    flexDirection: 'column',
    alignItems: 'center',
    justifyContent: 'center',
    padding: '40px',
    textAlign: 'center',
  },
  emptyIcon: {
    width: '60px',
    height: '60px',
    borderRadius: '50%',
    backgroundColor: '#f6f8fa',
    display: 'flex',
    alignItems: 'center',
    justifyContent: 'center',
    fontSize: '24px',
    color: '#8c959f',
    marginBottom: '16px',
  },
  emptyTitle: {
    margin: '0 0 8px 0',
    fontSize: '18px',
    color: '#24292f',
  },
  emptyText: {
    margin: 0,
    color: '#57606a',
    fontSize: '14px',
    maxWidth: '280px',
  },
  header: {
    padding: '16px',
    borderBottom: '1px solid #d0d7de',
    backgroundColor: '#f6f8fa',
  },
  headerTop: {
    display: 'flex',
    justifyContent: 'space-between',
    alignItems: 'center',
    marginBottom: '8px',
  },
  typeBadge: {
    fontSize: '10px',
    fontWeight: 600,
    textTransform: 'uppercase',
    padding: '3px 8px',
    borderRadius: '4px',
  },
  nodeCount: {
    fontSize: '12px',
    color: '#57606a',
  },
  title: {
    margin: '0 0 8px 0',
    fontSize: '16px',
    fontWeight: 600,
    color: '#24292f',
  },
  pivotBadge: {
    display: 'inline-block',
    fontSize: '11px',
    color: '#fb8500',
    backgroundColor: '#fff8e6',
    padding: '2px 8px',
    borderRadius: '4px',
    border: '1px solid #ffd699',
  },
  linksSection: {
    padding: '12px 16px',
    borderBottom: '1px solid #d0d7de',
    backgroundColor: '#fafbfc',
  },
  linksHeader: {
    fontSize: '11px',
    fontWeight: 600,
    color: '#57606a',
    textTransform: 'uppercase',
    marginBottom: '8px',
  },
  linksList: {
    display: 'flex',
    flexWrap: 'wrap',
    gap: '8px',
  },
  link: {
    fontSize: '12px',
    color: '#0969da',
    textDecoration: 'none',
    padding: '2px 6px',
    backgroundColor: '#ddf4ff',
    borderRadius: '4px',
  },
  moreLinks: {
    fontSize: '12px',
    color: '#57606a',
    padding: '2px 6px',
  },
  messages: {
    flex: 1,
    overflowY: 'auto',
    padding: '16px',
  },
  welcomeMessage: {
    textAlign: 'center',
    padding: '20px',
  },
  welcomeText: {
    color: '#57606a',
    fontSize: '14px',
    marginBottom: '16px',
  },
  suggestions: {
    display: 'flex',
    flexDirection: 'column',
    gap: '8px',
  },
  suggestionButton: {
    padding: '10px 14px',
    fontSize: '13px',
    backgroundColor: '#f6f8fa',
    color: '#24292f',
    border: '1px solid #d0d7de',
    borderRadius: '6px',
    cursor: 'pointer',
    textAlign: 'left',
    transition: 'all 0.15s',
  },
  message: {
    marginBottom: '16px',
    padding: '12px',
    borderRadius: '8px',
  },
  userMessage: {
    backgroundColor: '#ddf4ff',
    marginLeft: '20%',
  },
  assistantMessage: {
    backgroundColor: '#f6f8fa',
    marginRight: '10%',
  },
  messageHeader: {
    display: 'flex',
    justifyContent: 'space-between',
    marginBottom: '6px',
  },
  messageRole: {
    fontSize: '12px',
    fontWeight: 600,
    color: '#57606a',
  },
  messageTime: {
    fontSize: '11px',
    color: '#8c959f',
  },
  messageContent: {
    fontSize: '14px',
    color: '#24292f',
    lineHeight: 1.5,
  },
  loadingDots: {
    fontSize: '20px',
    color: '#57606a',
  },
  errorMessage: {
    padding: '12px',
    backgroundColor: '#ffebe9',
    border: '1px solid #ff8182',
    borderRadius: '6px',
    color: '#cf222e',
    fontSize: '13px',
    display: 'flex',
    flexDirection: 'column',
    gap: '8px',
  },
  errorContent: {
    lineHeight: 1.4,
  },
  retryButton: {
    alignSelf: 'flex-start',
    padding: '6px 12px',
    fontSize: '12px',
    fontWeight: 500,
    backgroundColor: '#ffffff',
    color: '#cf222e',
    border: '1px solid #cf222e',
    borderRadius: '4px',
    cursor: 'pointer',
    transition: 'all 0.15s',
  },
  cancelButton: {
    padding: '2px 8px',
    fontSize: '11px',
    backgroundColor: 'transparent',
    color: '#57606a',
    border: '1px solid #d0d7de',
    borderRadius: '4px',
    cursor: 'pointer',
    marginLeft: 'auto',
  },
  clearHistoryButton: {
    marginLeft: 'auto',
    padding: '2px 8px',
    fontSize: '10px',
    backgroundColor: 'transparent',
    color: '#8c959f',
    border: '1px solid #e1e4e8',
    borderRadius: '4px',
    cursor: 'pointer',
  },
  inputForm: {
    display: 'flex',
    gap: '8px',
    padding: '16px',
    borderTop: '1px solid #d0d7de',
    backgroundColor: '#f6f8fa',
  },
  input: {
    flex: 1,
    padding: '10px 14px',
    fontSize: '14px',
    border: '1px solid #d0d7de',
    borderRadius: '6px',
    outline: 'none',
    backgroundColor: '#ffffff',
  },
  sendButton: {
    padding: '10px 20px',
    fontSize: '14px',
    fontWeight: 600,
    backgroundColor: '#0969da',
    color: '#ffffff',
    border: 'none',
    borderRadius: '6px',
    cursor: 'pointer',
    transition: 'opacity 0.15s',
  },
};
