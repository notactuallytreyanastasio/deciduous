/**
 * ExplanationModal Component
 *
 * Full-screen modal for displaying AI-generated explanations.
 * Shows a single comprehensive explanation rather than a chat interface.
 */

import React, { useEffect, useRef } from 'react';
import ReactMarkdown from 'react-markdown';

interface ExplanationModalProps {
  isOpen: boolean;
  title: string;
  content: string | null;
  isLoading: boolean;
  error: string | null;
  onClose: () => void;
  timestamp?: Date;
}

export const ExplanationModal: React.FC<ExplanationModalProps> = ({
  isOpen,
  title,
  content,
  isLoading,
  error,
  onClose,
  timestamp,
}) => {
  const contentRef = useRef<HTMLDivElement>(null);

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

  // Scroll to top when content changes
  useEffect(() => {
    if (contentRef.current) {
      contentRef.current.scrollTop = 0;
    }
  }, [content]);

  if (!isOpen) return null;

  return (
    <div style={styles.overlay} onClick={onClose}>
      <div style={styles.modal} onClick={e => e.stopPropagation()}>
        {/* Header */}
        <div style={styles.header}>
          <div style={styles.headerLeft}>
            <span style={styles.claudeBadge}>Claude</span>
            {timestamp && (
              <span style={styles.timestamp}>
                {timestamp.toLocaleTimeString()}
              </span>
            )}
          </div>
          <button style={styles.closeBtn} onClick={onClose}>
            &times;
          </button>
        </div>

        {/* Title */}
        <h1 style={styles.title}>{title}</h1>

        {/* Content */}
        <div ref={contentRef} style={styles.content}>
          {isLoading && (
            <div style={styles.loading}>
              <div style={styles.spinner} />
              <span>Generating explanation...</span>
            </div>
          )}

          {error && (
            <div style={styles.error}>
              <strong>Error:</strong> {error}
            </div>
          )}

          {content && !isLoading && (
            <div style={styles.markdown}>
              <ReactMarkdown
                components={{
                  h1: ({ children }) => <h1 style={styles.h1}>{children}</h1>,
                  h2: ({ children }) => <h2 style={styles.h2}>{children}</h2>,
                  h3: ({ children }) => <h3 style={styles.h3}>{children}</h3>,
                  p: ({ children }) => <p style={styles.p}>{children}</p>,
                  ul: ({ children }) => <ul style={styles.ul}>{children}</ul>,
                  ol: ({ children }) => <ol style={styles.ol}>{children}</ol>,
                  li: ({ children }) => <li style={styles.li}>{children}</li>,
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
                }}
              >
                {content}
              </ReactMarkdown>
            </div>
          )}

          {!content && !isLoading && !error && (
            <div style={styles.empty}>
              <p>Enter a question to get an explanation about this narrative.</p>
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
  },
  headerLeft: {
    display: 'flex',
    alignItems: 'center',
    gap: '12px',
  },
  claudeBadge: {
    fontSize: '12px',
    fontWeight: 600,
    color: '#6741d9',
    backgroundColor: '#f3f0ff',
    padding: '4px 10px',
    borderRadius: '4px',
  },
  timestamp: {
    fontSize: '12px',
    color: '#8c959f',
  },
  closeBtn: {
    background: 'none',
    border: 'none',
    fontSize: '28px',
    cursor: 'pointer',
    color: '#57606a',
    padding: '0 8px',
    lineHeight: 1,
  },
  title: {
    margin: 0,
    padding: '24px 24px 16px',
    fontSize: '28px',
    fontWeight: 700,
    color: '#24292f',
    lineHeight: 1.3,
  },
  content: {
    flex: 1,
    overflow: 'auto',
    padding: '0 24px 24px',
  },
  loading: {
    display: 'flex',
    alignItems: 'center',
    gap: '12px',
    padding: '40px 0',
    justifyContent: 'center',
    color: '#57606a',
  },
  spinner: {
    width: '24px',
    height: '24px',
    border: '3px solid #e1e4e8',
    borderTopColor: '#0969da',
    borderRadius: '50%',
    animation: 'spin 1s linear infinite',
  },
  error: {
    padding: '16px',
    backgroundColor: '#ffebe9',
    borderRadius: '8px',
    color: '#cf222e',
  },
  empty: {
    padding: '40px 0',
    textAlign: 'center',
    color: '#57606a',
  },
  markdown: {
    lineHeight: 1.7,
  },
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
  p: {
    marginBottom: '16px',
    color: '#24292f',
  },
  ul: {
    marginBottom: '16px',
    paddingLeft: '24px',
  },
  ol: {
    marginBottom: '16px',
    paddingLeft: '24px',
  },
  li: {
    marginBottom: '8px',
    color: '#24292f',
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
    borderLeft: '4px solid #0969da',
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
};

// Add keyframes for spinner
const styleSheet = document.createElement('style');
styleSheet.textContent = `
  @keyframes spin {
    to { transform: rotate(360deg); }
  }
`;
document.head.appendChild(styleSheet);
