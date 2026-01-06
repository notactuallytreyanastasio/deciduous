import React, { useEffect } from 'react';
import Markdown from 'react-markdown';
import remarkGfm from 'remark-gfm';

interface AskResponseModalProps {
  isOpen: boolean;
  content: string;
  question: string;
  onClose: () => void;
  isLoading: boolean;
}

export const AskResponseModal: React.FC<AskResponseModalProps> = ({
  isOpen,
  content,
  question,
  onClose,
  isLoading,
}) => {
  // Close on Escape key
  useEffect(() => {
    const handleKeyDown = (e: KeyboardEvent) => {
      if (e.key === 'Escape') {
        onClose();
      }
    };
    if (isOpen) {
      document.addEventListener('keydown', handleKeyDown);
      return () => document.removeEventListener('keydown', handleKeyDown);
    }
  }, [isOpen, onClose]);

  if (!isOpen) return null;

  return (
    <div style={styles.backdrop} onClick={onClose}>
      <div style={styles.modal} onClick={e => e.stopPropagation()}>
        <div style={styles.header}>
          <h2 style={styles.title}>Claude Response</h2>
          <button onClick={onClose} style={styles.closeBtn}>×</button>
        </div>

        {question && (
          <div style={styles.questionSection}>
            <div style={styles.questionLabel}>Your question:</div>
            <div style={styles.questionText}>{question}</div>
          </div>
        )}

        <div style={styles.content}>
          {isLoading ? (
            <div style={styles.loading}>
              <div style={styles.spinner} />
              <span>Claude is thinking...</span>
            </div>
          ) : (
            <div className="ask-markdown" style={styles.markdown}>
              <Markdown remarkPlugins={[remarkGfm]}>{content}</Markdown>
            </div>
          )}
        </div>
      </div>
    </div>
  );
};

const styles: Record<string, React.CSSProperties> = {
  backdrop: {
    position: 'fixed',
    top: 0,
    left: 0,
    right: 0,
    bottom: 0,
    backgroundColor: 'rgba(0, 0, 0, 0.5)',
    display: 'flex',
    alignItems: 'center',
    justifyContent: 'center',
    zIndex: 100,
  },
  modal: {
    backgroundColor: '#ffffff',
    borderRadius: '12px',
    padding: '24px',
    width: '90%',
    maxWidth: '800px',
    maxHeight: '85vh',
    overflowY: 'auto',
    border: '1px solid #d0d7de',
    boxShadow: '0 8px 32px rgba(0, 0, 0, 0.15)',
  },
  header: {
    display: 'flex',
    justifyContent: 'space-between',
    alignItems: 'center',
    marginBottom: '16px',
    paddingBottom: '12px',
    borderBottom: '1px solid #d0d7de',
  },
  title: {
    margin: 0,
    fontSize: '18px',
    fontWeight: 600,
    color: '#24292f',
  },
  closeBtn: {
    width: '32px',
    height: '32px',
    border: 'none',
    background: '#f6f8fa',
    color: '#57606a',
    borderRadius: '6px',
    fontSize: '20px',
    cursor: 'pointer',
    display: 'flex',
    alignItems: 'center',
    justifyContent: 'center',
  },
  questionSection: {
    backgroundColor: '#f6f8fa',
    border: '1px solid #d0d7de',
    borderRadius: '8px',
    padding: '12px 16px',
    marginBottom: '16px',
  },
  questionLabel: {
    fontSize: '12px',
    fontWeight: 600,
    color: '#57606a',
    marginBottom: '4px',
    textTransform: 'uppercase',
    letterSpacing: '0.5px',
  },
  questionText: {
    fontSize: '14px',
    color: '#24292f',
    fontStyle: 'italic',
  },
  content: {
    fontSize: '14px',
    lineHeight: 1.6,
    color: '#24292f',
  },
  loading: {
    display: 'flex',
    flexDirection: 'column',
    alignItems: 'center',
    justifyContent: 'center',
    padding: '48px',
    gap: '16px',
    color: '#57606a',
  },
  spinner: {
    width: '32px',
    height: '32px',
    border: '3px solid #d0d7de',
    borderTopColor: '#0969da',
    borderRadius: '50%',
    animation: 'spin 0.8s linear infinite',
  },
  markdown: {
    lineHeight: 1.7,
  },
  codeBlock: {
    backgroundColor: '#f6f8fa',
    border: '1px solid #d0d7de',
    borderRadius: '6px',
    padding: '16px',
    overflow: 'auto',
    fontFamily: 'ui-monospace, SFMono-Regular, SF Mono, Menlo, Consolas, monospace',
    fontSize: '13px',
    lineHeight: 1.5,
    margin: '16px 0',
  },
  inlineCode: {
    backgroundColor: '#f6f8fa',
    padding: '2px 6px',
    borderRadius: '4px',
    fontFamily: 'ui-monospace, SFMono-Regular, SF Mono, Menlo, Consolas, monospace',
    fontSize: '13px',
  },
};

// Add keyframe animation and markdown styles via style tag
if (typeof document !== 'undefined') {
  const styleId = 'ask-modal-styles';
  if (!document.getElementById(styleId)) {
    const style = document.createElement('style');
    style.id = styleId;
    style.textContent = `
      @keyframes spin {
        to { transform: rotate(360deg); }
      }
      .ask-markdown h1 { font-size: 1.5em; font-weight: 600; margin: 1em 0 0.5em; border-bottom: 1px solid #d0d7de; padding-bottom: 0.3em; }
      .ask-markdown h2 { font-size: 1.3em; font-weight: 600; margin: 1em 0 0.5em; border-bottom: 1px solid #d0d7de; padding-bottom: 0.3em; }
      .ask-markdown h3 { font-size: 1.1em; font-weight: 600; margin: 1em 0 0.5em; }
      .ask-markdown h4 { font-size: 1em; font-weight: 600; margin: 1em 0 0.5em; }
      .ask-markdown p { margin: 0.5em 0; }
      .ask-markdown ul, .ask-markdown ol { margin: 0.5em 0; padding-left: 2em; }
      .ask-markdown li { margin: 0.25em 0; }
      .ask-markdown blockquote { margin: 0.5em 0; padding: 0.5em 1em; border-left: 4px solid #0969da; background: #f6f8fa; }
      .ask-markdown hr { border: none; border-top: 1px solid #d0d7de; margin: 1em 0; }
      .ask-markdown a { color: #0969da; text-decoration: none; }
      .ask-markdown a:hover { text-decoration: underline; }
      .ask-markdown strong { font-weight: 600; }
      .ask-markdown em { font-style: italic; }
      .ask-markdown table { border-collapse: collapse; margin: 1em 0; width: 100%; }
      .ask-markdown th, .ask-markdown td { border: 1px solid #d0d7de; padding: 8px 12px; text-align: left; }
      .ask-markdown th { background: #f6f8fa; font-weight: 600; }
      .ask-markdown pre { background: #f6f8fa; border: 1px solid #d0d7de; border-radius: 6px; padding: 16px; overflow: auto; font-family: ui-monospace, SFMono-Regular, SF Mono, Menlo, Consolas, monospace; font-size: 13px; line-height: 1.5; margin: 1em 0; }
      .ask-markdown code { background: #f6f8fa; padding: 2px 6px; border-radius: 4px; font-family: ui-monospace, SFMono-Regular, SF Mono, Menlo, Consolas, monospace; font-size: 13px; }
      .ask-markdown pre code { background: none; padding: 0; }
    `;
    document.head.appendChild(style);
  }
}

export default AskResponseModal;
