/**
 * PromptModal Component
 *
 * Modal for asking Claude about the code.
 * Shows question input, loading state, and Claude's response with markdown.
 */

import React, { useState, useEffect } from 'react';
import ReactMarkdown from 'react-markdown';
import remarkGfm from 'remark-gfm';

interface PromptModalProps {
  isOpen: boolean;
  narrativeName: string;
  content: string | null;
  isLoading: boolean;
  error: string | null;
  onAskClaude: (question: string) => void;
  onCancel: () => void;
  onClose: () => void;
  questionInputRef: React.RefObject<HTMLTextAreaElement>;
}

export const PromptModal: React.FC<PromptModalProps> = ({
  isOpen,
  narrativeName,
  content,
  isLoading,
  error,
  onAskClaude,
  onCancel,
  onClose,
  questionInputRef,
}) => {
  const [question, setQuestion] = useState('');

  // Handle escape key
  useEffect(() => {
    const handleKeyDown = (e: KeyboardEvent) => {
      if (e.key === 'Escape' && isOpen) {
        onClose();
      }
    };
    window.addEventListener('keydown', handleKeyDown);
    return () => window.removeEventListener('keydown', handleKeyDown);
  }, [isOpen, onClose]);

  // Reset question when modal opens
  useEffect(() => {
    if (isOpen) {
      setQuestion('');
    }
  }, [isOpen]);

  // Handle submit
  const handleSubmit = () => {
    if (question.trim() && !isLoading) {
      onAskClaude(question);
    }
  };

  if (!isOpen) return null;

  return (
    <div style={styles.overlay} onClick={onClose}>
      <div style={styles.modal} onClick={e => e.stopPropagation()}>
        {/* Header */}
        <div style={styles.header}>
          <h2 style={styles.headerTitle}>Ask About This Code</h2>
          <button style={styles.closeBtn} onClick={onClose}>
            &times;
          </button>
        </div>

        {/* Question Input - always visible at top */}
        <div style={styles.questionSection}>
          <label style={styles.label}>
            Your question about "{narrativeName}":
          </label>
          <textarea
            ref={questionInputRef}
            value={question}
            onChange={e => setQuestion(e.target.value)}
            onKeyDown={e => {
              if (e.key === 'Enter' && !e.shiftKey && !isLoading) {
                e.preventDefault();
                handleSubmit();
              }
            }}
            placeholder="What would you like to know? (e.g., Why was this approach chosen? What problem does this solve?)"
            style={styles.questionInput}
            rows={3}
            disabled={isLoading}
          />
          <button
            style={{
              ...styles.askButton,
              ...(!question.trim() || isLoading ? styles.askButtonDisabled : {}),
            }}
            onClick={handleSubmit}
            disabled={!question.trim() || isLoading}
          >
            {isLoading ? 'Asking Claude...' : 'Ask Claude'}
          </button>
        </div>

        {/* Response Area */}
        <div style={styles.responseSection}>
          {/* Loading */}
          {isLoading && (
            <div style={styles.loadingContainer}>
              <div style={styles.spinner} />
              <span style={styles.loadingText}>Claude is thinking...</span>
              <button style={styles.cancelButton} onClick={onCancel}>
                Cancel
              </button>
            </div>
          )}

          {/* Error */}
          {error && !isLoading && (
            <div style={styles.errorContainer}>
              <strong>Error:</strong> {error}
              <button
                style={styles.retryButton}
                onClick={handleSubmit}
              >
                Retry
              </button>
            </div>
          )}

          {/* Response Content */}
          {content && !isLoading && (
            <div style={styles.responseContent}>
              <div style={styles.responseBadge}>Claude's Response</div>
              <ReactMarkdown
                remarkPlugins={[remarkGfm]}
                components={{
                  h1: ({ children }) => <h1 style={styles.h1}>{children}</h1>,
                  h2: ({ children }) => <h2 style={styles.h2}>{children}</h2>,
                  h3: ({ children }) => <h3 style={styles.h3}>{children}</h3>,
                  h4: ({ children }) => <h4 style={styles.h4}>{children}</h4>,
                  p: ({ children }) => <p style={styles.p}>{children}</p>,
                  ul: ({ children }) => <ul style={styles.ul}>{children}</ul>,
                  ol: ({ children }) => <ol style={styles.ol}>{children}</ol>,
                  li: ({ children }) => <li style={styles.li}>{children}</li>,
                  table: ({ children }) => (
                    <div style={styles.tableWrapper}>
                      <table style={styles.table}>{children}</table>
                    </div>
                  ),
                  thead: ({ children }) => <thead style={styles.thead}>{children}</thead>,
                  tbody: ({ children }) => <tbody>{children}</tbody>,
                  tr: ({ children }) => <tr style={styles.tr}>{children}</tr>,
                  th: ({ children }) => <th style={styles.th}>{children}</th>,
                  td: ({ children }) => <td style={styles.td}>{children}</td>,
                  code: ({ className, children }) => {
                    const isInline = !className;
                    return isInline ? (
                      <code style={styles.inlineCode}>{children}</code>
                    ) : (
                      <pre style={styles.codeBlock}>
                        <code>{children}</code>
                      </pre>
                    );
                  },
                  blockquote: ({ children }) => (
                    <blockquote style={styles.blockquote}>{children}</blockquote>
                  ),
                  strong: ({ children }) => <strong style={styles.strong}>{children}</strong>,
                  hr: () => <hr style={styles.hr} />,
                  a: ({ href, children }) => (
                    <a href={href} target="_blank" rel="noopener noreferrer" style={styles.link}>
                      {children}
                    </a>
                  ),
                }}
              >
                {content}
              </ReactMarkdown>
            </div>
          )}

          {/* Empty State */}
          {!content && !isLoading && !error && (
            <div style={styles.emptyState}>
              <p>Enter your question above and click "Ask Claude" to get insights about this code.</p>
              <p style={styles.emptyHint}>
                Claude will analyze all {narrativeName ? `nodes in "${narrativeName}"` : 'nodes'} including their relationships, commits, and decisions.
              </p>
            </div>
          )}
        </div>
      </div>
    </div>
  );
};

const styles: Record<string, React.CSSProperties> = {
  overlay: {
    position: 'fixed',
    inset: 0,
    backgroundColor: 'rgba(0, 0, 0, 0.6)',
    display: 'flex',
    alignItems: 'center',
    justifyContent: 'center',
    zIndex: 2000,
    padding: '40px',
  },
  modal: {
    backgroundColor: '#ffffff',
    borderRadius: '12px',
    width: '100%',
    maxWidth: '900px',
    maxHeight: '90vh',
    display: 'flex',
    flexDirection: 'column',
    boxShadow: '0 20px 60px rgba(0,0,0,0.3)',
    overflow: 'hidden',
  },
  header: {
    display: 'flex',
    alignItems: 'center',
    justifyContent: 'space-between',
    padding: '16px 24px',
    borderBottom: '1px solid #e1e4e8',
    backgroundColor: '#e91e8c',
  },
  headerTitle: {
    margin: 0,
    fontSize: '20px',
    fontWeight: 700,
    color: '#ffffff',
  },
  closeBtn: {
    background: 'none',
    border: 'none',
    fontSize: '28px',
    cursor: 'pointer',
    color: '#ffffff',
    padding: '0 8px',
    lineHeight: 1,
  },
  questionSection: {
    padding: '20px 24px',
    borderBottom: '1px solid #e1e4e8',
    backgroundColor: '#fafbfc',
  },
  label: {
    display: 'block',
    fontSize: '14px',
    fontWeight: 600,
    color: '#24292f',
    marginBottom: '8px',
  },
  questionInput: {
    width: '100%',
    padding: '12px 14px',
    fontSize: '14px',
    border: '1px solid #d0d7de',
    borderRadius: '8px',
    outline: 'none',
    backgroundColor: '#ffffff',
    resize: 'none',
    fontFamily: 'inherit',
    lineHeight: 1.5,
    marginBottom: '12px',
    boxSizing: 'border-box',
  },
  askButton: {
    padding: '12px 24px',
    fontSize: '14px',
    fontWeight: 600,
    backgroundColor: '#e91e8c',
    color: '#ffffff',
    border: 'none',
    borderRadius: '8px',
    cursor: 'pointer',
    transition: 'all 0.2s ease',
  },
  askButtonDisabled: {
    backgroundColor: '#d0d7de',
    cursor: 'not-allowed',
  },
  responseSection: {
    flex: 1,
    overflow: 'auto',
    minHeight: '300px',
  },
  loadingContainer: {
    display: 'flex',
    flexDirection: 'column',
    alignItems: 'center',
    justifyContent: 'center',
    padding: '60px 24px',
    gap: '16px',
  },
  spinner: {
    width: '40px',
    height: '40px',
    border: '4px solid #e1e4e8',
    borderTopColor: '#e91e8c',
    borderRadius: '50%',
    animation: 'spin 1s linear infinite',
  },
  loadingText: {
    fontSize: '16px',
    color: '#57606a',
  },
  cancelButton: {
    padding: '8px 20px',
    fontSize: '13px',
    fontWeight: 500,
    backgroundColor: '#ffffff',
    color: '#57606a',
    border: '1px solid #d0d7de',
    borderRadius: '6px',
    cursor: 'pointer',
  },
  errorContainer: {
    margin: '24px',
    padding: '16px',
    backgroundColor: '#ffebe9',
    borderRadius: '8px',
    color: '#cf222e',
    display: 'flex',
    flexDirection: 'column',
    gap: '12px',
  },
  retryButton: {
    alignSelf: 'flex-start',
    padding: '8px 16px',
    fontSize: '13px',
    fontWeight: 500,
    backgroundColor: '#ffffff',
    color: '#cf222e',
    border: '1px solid #cf222e',
    borderRadius: '6px',
    cursor: 'pointer',
  },
  responseContent: {
    padding: '24px',
    fontSize: '14px',
    lineHeight: 1.6,
  },
  responseBadge: {
    display: 'inline-block',
    fontSize: '12px',
    fontWeight: 600,
    color: '#e91e8c',
    backgroundColor: '#fce7f3',
    padding: '4px 10px',
    borderRadius: '4px',
    marginBottom: '16px',
  },
  emptyState: {
    padding: '60px 24px',
    textAlign: 'center',
    color: '#57606a',
  },
  emptyHint: {
    fontSize: '13px',
    color: '#8c959f',
    marginTop: '8px',
  },
  // Markdown styles
  h1: {
    fontSize: '24px',
    fontWeight: 700,
    color: '#24292f',
    marginTop: '24px',
    marginBottom: '12px',
    paddingBottom: '8px',
    borderBottom: '1px solid #e1e4e8',
  },
  h2: {
    fontSize: '20px',
    fontWeight: 600,
    color: '#24292f',
    marginTop: '20px',
    marginBottom: '10px',
  },
  h3: {
    fontSize: '16px',
    fontWeight: 600,
    color: '#24292f',
    marginTop: '16px',
    marginBottom: '8px',
  },
  h4: {
    fontSize: '14px',
    fontWeight: 600,
    color: '#24292f',
    marginTop: '12px',
    marginBottom: '6px',
  },
  p: {
    marginBottom: '12px',
    color: '#24292f',
  },
  ul: {
    marginBottom: '12px',
    paddingLeft: '24px',
  },
  ol: {
    marginBottom: '12px',
    paddingLeft: '24px',
  },
  li: {
    marginBottom: '4px',
    color: '#24292f',
  },
  // Table styles
  tableWrapper: {
    overflowX: 'auto',
    marginBottom: '16px',
  },
  table: {
    borderCollapse: 'collapse',
    width: '100%',
    fontSize: '13px',
  },
  thead: {
    backgroundColor: '#f6f8fa',
  },
  tr: {
    borderBottom: '1px solid #d0d7de',
  },
  th: {
    padding: '8px 12px',
    textAlign: 'left',
    fontWeight: 600,
    color: '#24292f',
    borderBottom: '2px solid #d0d7de',
    whiteSpace: 'nowrap',
  },
  td: {
    padding: '8px 12px',
    textAlign: 'left',
    color: '#24292f',
    verticalAlign: 'top',
  },
  inlineCode: {
    backgroundColor: '#f6f8fa',
    padding: '2px 6px',
    borderRadius: '4px',
    fontFamily: 'monospace',
    fontSize: '0.9em',
    color: '#0969da',
  },
  codeBlock: {
    backgroundColor: '#f6f8fa',
    padding: '16px',
    borderRadius: '8px',
    overflow: 'auto',
    fontFamily: 'monospace',
    fontSize: '13px',
    marginBottom: '16px',
  },
  blockquote: {
    borderLeft: '4px solid #e91e8c',
    paddingLeft: '16px',
    marginLeft: 0,
    color: '#57606a',
    fontStyle: 'italic',
    marginBottom: '16px',
  },
  strong: {
    fontWeight: 600,
    color: '#24292f',
  },
  hr: {
    border: 'none',
    borderTop: '2px solid #e1e4e8',
    margin: '20px 0',
  },
  link: {
    color: '#0969da',
    textDecoration: 'none',
  },
};

// Add keyframes for spinner
const styleSheet = document.createElement('style');
styleSheet.textContent = `
  @keyframes spin {
    to { transform: rotate(360deg); }
  }
`;
document.head.appendChild(styleSheet);
