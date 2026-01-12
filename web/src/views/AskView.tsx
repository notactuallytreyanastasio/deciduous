/**
 * Ask View - AI-First Decision Graph Explorer
 *
 * Ask questions about your project history. The AI analyzes the graph,
 * finds relevant nodes, and explains what it found in a narrative format.
 *
 * Two-panel result display:
 * - Left: Stacked node cards you can explore
 * - Right: Markdown explanation of the answer
 */

import React, { useState, useMemo } from 'react';
import { Link } from 'react-router-dom';
import ReactMarkdown from 'react-markdown';
import type { GraphData, DecisionNode, GitCommit } from '../types/graph';
import { truncate, getConfidence, getPrompt } from '../types/graph';
import { getNodeColor } from '../utils/colors';

interface AskViewProps {
  graphData: GraphData;
  gitHistory?: GitCommit[];
}

interface AnalysisResult {
  nodes: DecisionNode[];
  markdown: string;
  query: string;
}

// Analyze the graph based on a question
function analyzeGraph(
  question: string,
  graphData: GraphData,
  gitHistory: GitCommit[]
): AnalysisResult {
  const q = question.toLowerCase();
  const { nodes, edges } = graphData;

  // Build connection counts for importance ranking
  const connectionCount = new Map<number, number>();
  nodes.forEach(n => connectionCount.set(n.id, 0));
  edges.forEach(e => {
    connectionCount.set(e.from_node_id, (connectionCount.get(e.from_node_id) || 0) + 1);
    connectionCount.set(e.to_node_id, (connectionCount.get(e.to_node_id) || 0) + 1);
  });

  // Find parent/child relationships
  const children = new Map<number, number[]>();
  const parents = new Map<number, number[]>();
  nodes.forEach(n => {
    children.set(n.id, []);
    parents.set(n.id, []);
  });
  edges.forEach(e => {
    children.get(e.from_node_id)?.push(e.to_node_id);
    parents.get(e.to_node_id)?.push(e.from_node_id);
  });

  // Determine query intent
  const isAboutFeatures = /feature|critical|important|major|key|significant/i.test(q);
  const isAboutDecisions = /decision|choice|chose|pick|select/i.test(q);
  const isAboutGoals = /goal|objective|aim|target|purpose/i.test(q);
  const isAboutProblems = /problem|issue|bug|error|fail|challenge/i.test(q);
  const isAboutRecent = /recent|latest|new|last|current/i.test(q);
  const isAboutHistory = /history|evolution|progress|journey|timeline/i.test(q);
  const isAboutOutcomes = /outcome|result|success|complete|finish/i.test(q);

  // Score and rank nodes
  const scoredNodes = nodes.map(node => {
    let score = 0;
    const title = node.title.toLowerCase();
    const confidence = getConfidence(node) || 50;

    // Connection importance
    score += (connectionCount.get(node.id) || 0) * 10;

    // Type relevance based on query
    if (isAboutGoals && node.node_type === 'goal') score += 50;
    if (isAboutDecisions && node.node_type === 'decision') score += 50;
    if (isAboutOutcomes && node.node_type === 'outcome') score += 50;
    if (isAboutFeatures && (node.node_type === 'goal' || node.node_type === 'action')) score += 30;

    // Goals and outcomes are generally important
    if (node.node_type === 'goal') score += 20;
    if (node.node_type === 'outcome') score += 15;

    // High confidence = more decisive
    score += confidence / 5;

    // Keyword matching in title
    const keywords = q.split(/\s+/).filter(w => w.length > 3);
    keywords.forEach(kw => {
      if (title.includes(kw)) score += 25;
    });

    // Recency bonus if asking about recent
    if (isAboutRecent) {
      const age = Date.now() - new Date(node.created_at).getTime();
      const dayAge = age / (1000 * 60 * 60 * 24);
      if (dayAge < 7) score += 40;
      else if (dayAge < 30) score += 20;
    }

    // Has prompt = captured user intent
    if (getPrompt(node)) score += 10;

    return { node, score };
  });

  // Sort by score and take top results
  scoredNodes.sort((a, b) => b.score - a.score);
  const topNodes = scoredNodes.slice(0, 12).map(s => s.node);

  // Generate markdown explanation
  const markdown = generateExplanation(question, topNodes, graphData, gitHistory, {
    isAboutFeatures,
    isAboutDecisions,
    isAboutGoals,
    isAboutProblems,
    isAboutRecent,
    isAboutHistory,
    isAboutOutcomes,
  });

  return { nodes: topNodes, markdown, query: question };
}

function generateExplanation(
  _question: string,
  nodes: DecisionNode[],
  graphData: GraphData,
  gitHistory: GitCommit[],
  intent: Record<string, boolean>
): string {
  if (nodes.length === 0) {
    return `## No Results Found\n\nI couldn't find any nodes in the decision graph that match your question. Try rephrasing or asking about specific features, goals, or decisions.`;
  }

  const goals = nodes.filter(n => n.node_type === 'goal');
  const decisions = nodes.filter(n => n.node_type === 'decision');
  const actions = nodes.filter(n => n.node_type === 'action');
  const outcomes = nodes.filter(n => n.node_type === 'outcome');

  let md = `## Analysis\n\n`;

  // Opening based on intent
  if (intent.isAboutFeatures || intent.isAboutHistory) {
    md += `Based on the decision graph, here are the most significant elements I found:\n\n`;
  } else if (intent.isAboutGoals) {
    md += `I found ${goals.length} goals that are relevant to your question:\n\n`;
  } else if (intent.isAboutDecisions) {
    md += `Here are the key decisions I identified:\n\n`;
  } else if (intent.isAboutRecent) {
    md += `Looking at recent activity in the project:\n\n`;
  } else {
    md += `Here's what I found in the decision history:\n\n`;
  }

  // Summarize goals
  if (goals.length > 0) {
    md += `### Key Goals\n\n`;
    goals.slice(0, 5).forEach(g => {
      const conf = getConfidence(g);
      md += `- **${g.title}**`;
      if (conf) md += ` _(${conf}% confidence)_`;
      md += `\n`;
    });
    md += `\n`;
  }

  // Summarize decisions
  if (decisions.length > 0) {
    md += `### Important Decisions\n\n`;
    decisions.slice(0, 5).forEach(d => {
      md += `- ${d.title}\n`;
    });
    md += `\n`;
  }

  // Summarize outcomes
  if (outcomes.length > 0) {
    md += `### Outcomes\n\n`;
    outcomes.slice(0, 4).forEach(o => {
      md += `- ${o.title}\n`;
    });
    md += `\n`;
  }

  // Summarize actions if relevant
  if (actions.length > 0 && (intent.isAboutFeatures || intent.isAboutHistory)) {
    md += `### Key Actions\n\n`;
    actions.slice(0, 4).forEach(a => {
      md += `- ${a.title}\n`;
    });
    md += `\n`;
  }

  // Add some statistics
  md += `---\n\n`;
  md += `**Graph Context:** ${graphData.nodes.length} total nodes, ${graphData.edges.length} connections`;
  if (gitHistory.length > 0) {
    md += `, ${gitHistory.length} commits tracked`;
  }
  md += `\n\n`;

  // Guidance
  md += `_Explore the cards on the left to dive deeper into each node. Click to expand and see connections._`;

  return md;
}

// Example questions to help users get started
const EXAMPLE_QUESTIONS = [
  "What have been the most critical features in this project's development?",
  "Show me the major goals and their outcomes",
  "What decisions led to the current architecture?",
  "What happened recently in the project?",
  "What are the key milestones achieved so far?",
];

export const AskView: React.FC<AskViewProps> = ({
  graphData,
  gitHistory = [],
}) => {
  const [question, setQuestion] = useState('');
  const [result, setResult] = useState<AnalysisResult | null>(null);
  const [expandedNodes, setExpandedNodes] = useState<Set<number>>(new Set());
  const [isAnalyzing, setIsAnalyzing] = useState(false);

  // Build parent/child maps for exploration
  const { children, parents } = useMemo(() => {
    const children = new Map<number, number[]>();
    const parents = new Map<number, number[]>();
    graphData.nodes.forEach(n => {
      children.set(n.id, []);
      parents.set(n.id, []);
    });
    graphData.edges.forEach(e => {
      children.get(e.from_node_id)?.push(e.to_node_id);
      parents.get(e.to_node_id)?.push(e.from_node_id);
    });
    return { children, parents };
  }, [graphData]);

  const handleAsk = () => {
    if (!question.trim()) return;

    setIsAnalyzing(true);
    // Simulate async analysis
    setTimeout(() => {
      const analysis = analyzeGraph(question, graphData, gitHistory);
      setResult(analysis);
      setExpandedNodes(new Set());
      setIsAnalyzing(false);
    }, 300);
  };

  const handleExampleClick = (q: string) => {
    setQuestion(q);
    setIsAnalyzing(true);
    setTimeout(() => {
      const analysis = analyzeGraph(q, graphData, gitHistory);
      setResult(analysis);
      setExpandedNodes(new Set());
      setIsAnalyzing(false);
    }, 300);
  };

  const toggleExpand = (nodeId: number) => {
    const next = new Set(expandedNodes);
    if (next.has(nodeId)) {
      next.delete(nodeId);
    } else {
      next.add(nodeId);
    }
    setExpandedNodes(next);
  };

  const getNodeById = (id: number) => graphData.nodes.find(n => n.id === id);

  return (
    <div style={styles.container}>
      {/* Header */}
      <header style={styles.header}>
        <div style={styles.headerLeft}>
          <h1 style={styles.logo}>Deciduous</h1>
          <span style={styles.headerDivider}>/</span>
          <span style={styles.headerPage}>Ask</span>
        </div>
        <Link to="/" style={styles.backLink}>
          ← Back to Graph
        </Link>
      </header>

      {/* Question input area */}
      <div style={styles.questionArea}>
        <h2 style={styles.title}>Ask your decision graph</h2>
        <p style={styles.subtitle}>
          Ask questions about your project's history, goals, decisions, and outcomes.
        </p>

        <div style={styles.inputArea}>
          <textarea
            value={question}
            onChange={(e) => setQuestion(e.target.value)}
            onKeyDown={(e) => {
              if (e.key === 'Enter' && (e.metaKey || e.ctrlKey)) {
                handleAsk();
              }
            }}
            placeholder="What would you like to know about this project?

Ask complex questions like:
• What were the key architectural decisions and why were they made?
• How did the project evolve from its initial goals?
• What challenges came up and how were they resolved?"
            style={styles.textarea}
          />
          <div style={styles.inputFooter}>
            <span style={styles.inputHint}>Press ⌘+Enter to ask</span>
            <button
              onClick={handleAsk}
              disabled={!question.trim() || isAnalyzing}
              style={{
                ...styles.askButton,
                opacity: !question.trim() || isAnalyzing ? 0.5 : 1,
              }}
            >
              {isAnalyzing ? 'Analyzing...' : 'Ask'}
            </button>
          </div>
        </div>

        {/* Example questions */}
        {!result && (
          <div style={styles.examples}>
            <span style={styles.examplesLabel}>Try asking:</span>
            <div style={styles.exampleButtons}>
              {EXAMPLE_QUESTIONS.map((q, i) => (
                <button
                  key={i}
                  onClick={() => handleExampleClick(q)}
                  style={styles.exampleButton}
                >
                  {q}
                </button>
              ))}
            </div>
          </div>
        )}
      </div>

      {/* Results area */}
      {result && (
        <div style={styles.resultsArea}>
          {/* Left: Node cards */}
          <div style={styles.nodesPanel}>
            <div style={styles.panelHeader}>
              <h3 style={styles.panelTitle}>Related Nodes</h3>
              <span style={styles.nodeCount}>{result.nodes.length} found</span>
            </div>

            <div style={styles.nodeStack}>
              {result.nodes.map((node) => {
                const isExpanded = expandedNodes.has(node.id);
                const nodeChildren = children.get(node.id) || [];
                const nodeParents = parents.get(node.id) || [];
                const hasConnections = nodeChildren.length > 0 || nodeParents.length > 0;

                return (
                  <div key={node.id} style={styles.nodeCardWrapper}>
                    <div
                      style={{
                        ...styles.nodeCard,
                        borderLeftColor: getNodeColor(node.node_type),
                      }}
                      onClick={() => hasConnections && toggleExpand(node.id)}
                    >
                      <div style={styles.nodeCardHeader}>
                        <span
                          style={{
                            ...styles.nodeType,
                            backgroundColor: getNodeColor(node.node_type) + '22',
                            color: getNodeColor(node.node_type),
                          }}
                        >
                          {node.node_type}
                        </span>
                        <span style={styles.nodeId}>#{node.id}</span>
                      </div>

                      <div style={styles.nodeTitle}>{node.title}</div>

                      <div style={styles.nodeFooter}>
                        <span style={styles.nodeDate}>
                          {new Date(node.created_at).toLocaleDateString()}
                        </span>
                        {hasConnections && (
                          <span style={styles.expandHint}>
                            {isExpanded ? '▼' : '▶'} {nodeParents.length + nodeChildren.length} connections
                          </span>
                        )}
                      </div>

                      {getPrompt(node) && (
                        <div style={styles.nodePrompt}>
                          "{truncate(getPrompt(node) || '', 100)}"
                        </div>
                      )}
                    </div>

                    {/* Expanded connections */}
                    {isExpanded && (
                      <div style={styles.connections}>
                        {nodeParents.length > 0 && (
                          <div style={styles.connectionGroup}>
                            <span style={styles.connectionLabel}>↑ Parents</span>
                            {nodeParents.map(pid => {
                              const parent = getNodeById(pid);
                              if (!parent) return null;
                              return (
                                <div
                                  key={pid}
                                  style={{
                                    ...styles.connectionCard,
                                    borderLeftColor: getNodeColor(parent.node_type),
                                  }}
                                >
                                  <span style={styles.connType}>{parent.node_type}</span>
                                  <span style={styles.connTitle}>{truncate(parent.title, 50)}</span>
                                </div>
                              );
                            })}
                          </div>
                        )}

                        {nodeChildren.length > 0 && (
                          <div style={styles.connectionGroup}>
                            <span style={styles.connectionLabel}>↓ Children</span>
                            {nodeChildren.map(cid => {
                              const child = getNodeById(cid);
                              if (!child) return null;
                              return (
                                <div
                                  key={cid}
                                  style={{
                                    ...styles.connectionCard,
                                    borderLeftColor: getNodeColor(child.node_type),
                                  }}
                                >
                                  <span style={styles.connType}>{child.node_type}</span>
                                  <span style={styles.connTitle}>{truncate(child.title, 50)}</span>
                                </div>
                              );
                            })}
                          </div>
                        )}
                      </div>
                    )}
                  </div>
                );
              })}
            </div>
          </div>

          {/* Right: Markdown explanation */}
          <div style={styles.explanationPanel}>
            <div style={styles.panelHeader}>
              <h3 style={styles.panelTitle}>Explanation</h3>
            </div>

            <div style={styles.markdown}>
              <ReactMarkdown>{result.markdown}</ReactMarkdown>
            </div>

            {/* Ask another question */}
            <div style={styles.followUp}>
              <button
                onClick={() => {
                  setResult(null);
                  setQuestion('');
                }}
                style={styles.newQuestionButton}
              >
                Ask another question
              </button>
            </div>
          </div>
        </div>
      )}
    </div>
  );
};

// =============================================================================
// Styles
// =============================================================================

const styles: Record<string, React.CSSProperties> = {
  container: {
    height: '100vh',
    display: 'flex',
    flexDirection: 'column',
    backgroundColor: '#ffffff',
  },

  // Header
  header: {
    display: 'flex',
    justifyContent: 'space-between',
    alignItems: 'center',
    padding: '16px 32px',
    borderBottom: '1px solid #d0d7de',
    backgroundColor: '#f6f8fa',
  },
  headerLeft: {
    display: 'flex',
    alignItems: 'center',
    gap: '12px',
  },
  logo: {
    margin: 0,
    fontSize: '18px',
    fontWeight: 600,
    color: '#24292f',
  },
  headerDivider: {
    color: '#d0d7de',
    fontSize: '18px',
  },
  headerPage: {
    fontSize: '18px',
    color: '#57606a',
  },
  backLink: {
    fontSize: '14px',
    color: '#0969da',
    textDecoration: 'none',
  },

  // Question area
  questionArea: {
    padding: '40px 60px',
    borderBottom: '1px solid #d0d7de',
    backgroundColor: '#f6f8fa',
  },
  title: {
    margin: '0 0 8px 0',
    fontSize: '28px',
    fontWeight: 600,
    color: '#24292f',
  },
  subtitle: {
    margin: '0 0 24px 0',
    fontSize: '16px',
    color: '#57606a',
  },
  inputArea: {
    maxWidth: '900px',
  },
  textarea: {
    width: '100%',
    minHeight: '140px',
    padding: '16px 20px',
    fontSize: '16px',
    lineHeight: '1.5',
    border: '1px solid #d0d7de',
    borderRadius: '8px',
    outline: 'none',
    backgroundColor: '#ffffff',
    resize: 'vertical',
    fontFamily: 'inherit',
  },
  inputFooter: {
    display: 'flex',
    justifyContent: 'space-between',
    alignItems: 'center',
    marginTop: '12px',
  },
  inputHint: {
    fontSize: '13px',
    color: '#8c959f',
  },
  askButton: {
    padding: '12px 28px',
    fontSize: '15px',
    fontWeight: 600,
    backgroundColor: '#0969da',
    color: '#ffffff',
    border: 'none',
    borderRadius: '8px',
    cursor: 'pointer',
    transition: 'background-color 0.15s',
  },

  // Examples
  examples: {
    marginTop: '24px',
  },
  examplesLabel: {
    fontSize: '13px',
    color: '#57606a',
    marginRight: '12px',
  },
  exampleButtons: {
    display: 'flex',
    flexWrap: 'wrap',
    gap: '8px',
    marginTop: '8px',
  },
  exampleButton: {
    padding: '8px 14px',
    fontSize: '13px',
    backgroundColor: '#ffffff',
    color: '#57606a',
    border: '1px solid #d0d7de',
    borderRadius: '20px',
    cursor: 'pointer',
    transition: 'all 0.15s',
  },

  // Results
  resultsArea: {
    flex: 1,
    display: 'flex',
    overflow: 'hidden',
  },

  // Nodes panel
  nodesPanel: {
    width: '45%',
    borderRight: '1px solid #d0d7de',
    display: 'flex',
    flexDirection: 'column',
    overflow: 'hidden',
  },
  panelHeader: {
    padding: '16px 24px',
    borderBottom: '1px solid #d0d7de',
    backgroundColor: '#f6f8fa',
    display: 'flex',
    justifyContent: 'space-between',
    alignItems: 'center',
  },
  panelTitle: {
    margin: 0,
    fontSize: '16px',
    fontWeight: 600,
    color: '#24292f',
  },
  nodeCount: {
    fontSize: '13px',
    color: '#57606a',
  },
  nodeStack: {
    flex: 1,
    overflow: 'auto',
    padding: '16px 24px',
  },
  nodeCardWrapper: {
    marginBottom: '12px',
  },
  nodeCard: {
    padding: '16px',
    backgroundColor: '#ffffff',
    border: '1px solid #d0d7de',
    borderLeft: '4px solid',
    borderRadius: '8px',
    cursor: 'pointer',
    transition: 'box-shadow 0.15s',
  },
  nodeCardHeader: {
    display: 'flex',
    justifyContent: 'space-between',
    alignItems: 'center',
    marginBottom: '8px',
  },
  nodeType: {
    fontSize: '11px',
    fontWeight: 600,
    textTransform: 'uppercase',
    padding: '3px 8px',
    borderRadius: '4px',
  },
  nodeId: {
    fontSize: '12px',
    color: '#8c959f',
    fontFamily: 'monospace',
  },
  nodeTitle: {
    fontSize: '14px',
    fontWeight: 500,
    color: '#24292f',
    lineHeight: 1.4,
    marginBottom: '8px',
  },
  nodeFooter: {
    display: 'flex',
    justifyContent: 'space-between',
    alignItems: 'center',
  },
  nodeDate: {
    fontSize: '12px',
    color: '#8c959f',
  },
  expandHint: {
    fontSize: '12px',
    color: '#0969da',
  },
  nodePrompt: {
    marginTop: '12px',
    padding: '10px 12px',
    backgroundColor: '#f6f8fa',
    borderRadius: '6px',
    fontSize: '13px',
    color: '#57606a',
    fontStyle: 'italic',
    lineHeight: 1.4,
  },

  // Connections
  connections: {
    marginTop: '8px',
    marginLeft: '16px',
    paddingLeft: '16px',
    borderLeft: '2px solid #d0d7de',
  },
  connectionGroup: {
    marginBottom: '12px',
  },
  connectionLabel: {
    display: 'block',
    fontSize: '11px',
    fontWeight: 600,
    color: '#8c959f',
    textTransform: 'uppercase',
    marginBottom: '6px',
  },
  connectionCard: {
    display: 'flex',
    alignItems: 'center',
    gap: '10px',
    padding: '8px 12px',
    backgroundColor: '#f6f8fa',
    borderLeft: '3px solid',
    borderRadius: '4px',
    marginBottom: '4px',
  },
  connType: {
    fontSize: '10px',
    fontWeight: 600,
    color: '#57606a',
    textTransform: 'uppercase',
  },
  connTitle: {
    fontSize: '13px',
    color: '#24292f',
  },

  // Explanation panel
  explanationPanel: {
    flex: 1,
    display: 'flex',
    flexDirection: 'column',
    overflow: 'hidden',
  },
  markdown: {
    flex: 1,
    overflow: 'auto',
    padding: '24px 32px',
    fontSize: '15px',
    lineHeight: 1.6,
    color: '#24292f',
  },
  followUp: {
    padding: '16px 32px',
    borderTop: '1px solid #d0d7de',
    backgroundColor: '#f6f8fa',
  },
  newQuestionButton: {
    padding: '10px 20px',
    fontSize: '14px',
    backgroundColor: '#ffffff',
    color: '#24292f',
    border: '1px solid #d0d7de',
    borderRadius: '6px',
    cursor: 'pointer',
  },
};
