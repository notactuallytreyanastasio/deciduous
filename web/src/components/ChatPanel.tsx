/**
 * ChatPanel Component
 *
 * Chat interface for asking questions about a selected narrative.
 * Connects to /api/ask to shell out to Claude.
 */

import React, { useState, useRef, useEffect } from 'react';
import ReactMarkdown from 'react-markdown';
import type { Narrative, ChatMessage, NarrativeContext, ArchaeologyAskContext } from '../types/archaeology';
import { formatNarrativeContext } from '../utils/archaeologyProcessing';
import { getNodeColor } from '../utils/colors';

interface ChatPanelProps {
  narrative: Narrative | null;
  onSelectNode?: (nodeId: number) => void;
}

// Generate unique ID for messages
const generateId = () => Math.random().toString(36).substring(2, 9);

export const ChatPanel: React.FC<ChatPanelProps> = ({
  narrative,
}) => {
  const [messages, setMessages] = useState<ChatMessage[]>([]);
  const [input, setInput] = useState('');
  const [isLoading, setIsLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const messagesEndRef = useRef<HTMLDivElement>(null);

  // Scroll to bottom when messages change
  useEffect(() => {
    messagesEndRef.current?.scrollIntoView({ behavior: 'smooth' });
  }, [messages]);

  // Clear messages when narrative changes
  useEffect(() => {
    setMessages([]);
    setError(null);
  }, [narrative?.id]);

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
    setIsLoading(true);
    setError(null);

    try {
      // Build context for the API
      const narrativeContext: NarrativeContext = formatNarrativeContext(narrative);
      const askContext: ArchaeologyAskContext = {
        narrative: narrativeContext,
        visible_node_ids: narrative.nodes.map(n => n.id),
      };

      const response = await fetch('/api/ask', {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({
          question: userMessage.content,
          context: askContext,
        }),
      });

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
      const errorMsg = err instanceof Error ? err.message : 'Unknown error';
      setError(errorMsg);
    } finally {
      setIsLoading(false);
    }
  };

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
            </div>
            <div style={styles.loadingDots}>
              <span>.</span><span>.</span><span>.</span>
            </div>
          </div>
        )}

        {error && (
          <div style={styles.errorMessage}>
            <strong>Error:</strong> {error}
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
