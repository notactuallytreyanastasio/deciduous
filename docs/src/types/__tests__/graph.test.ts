import { describe, it, expect } from 'vitest';
import {
  parseMetadata,
  getConfidence,
  getCommit,
  shortCommit,
  githubCommitUrl,
  getConfidenceLevel,
  parseNode,
  type DecisionNode,
} from '../graph';

describe('graph type helpers', () => {
  describe('parseMetadata', () => {
    it('returns null for null input', () => {
      expect(parseMetadata(null)).toBeNull();
    });

    it('returns null for empty string', () => {
      expect(parseMetadata('')).toBeNull();
    });

    it('parses valid JSON with confidence', () => {
      const result = parseMetadata('{"confidence": 85}');
      expect(result).toEqual({ confidence: 85 });
    });

    it('parses JSON with additional metadata', () => {
      const result = parseMetadata('{"confidence": 70, "commit": "abc123"}');
      expect(result).toEqual({ confidence: 70, commit: 'abc123' });
    });

    it('returns null for invalid JSON', () => {
      expect(parseMetadata('not json')).toBeNull();
      expect(parseMetadata('{invalid')).toBeNull();
    });
  });

  describe('getConfidence', () => {
    const makeNode = (metadata: string | null): DecisionNode => ({
      id: 1,
      node_type: 'decision',
      title: 'Test',
      description: null,
      status: 'pending',
      created_at: '2024-01-01T00:00:00Z',
      updated_at: '2024-01-01T00:00:00Z',
      metadata_json: metadata,
    });

    it('returns null when no metadata', () => {
      expect(getConfidence(makeNode(null))).toBeNull();
    });

    it('extracts confidence from metadata', () => {
      expect(getConfidence(makeNode('{"confidence": 95}'))).toBe(95);
    });

    it('returns null when metadata has no confidence', () => {
      expect(getConfidence(makeNode('{"other": "data"}'))).toBeNull();
    });
  });

  describe('getCommit', () => {
    const makeNode = (metadata: string | null): DecisionNode => ({
      id: 1,
      node_type: 'action',
      title: 'Test',
      description: null,
      status: 'completed',
      created_at: '2024-01-01T00:00:00Z',
      updated_at: '2024-01-01T00:00:00Z',
      metadata_json: metadata,
    });

    it('returns null when no metadata', () => {
      expect(getCommit(makeNode(null))).toBeNull();
    });

    it('extracts commit hash from metadata', () => {
      expect(getCommit(makeNode('{"commit": "abc123def456"}'))).toBe('abc123def456');
    });

    it('extracts commit when both confidence and commit present', () => {
      expect(getCommit(makeNode('{"confidence": 85, "commit": "5a2ae23"}'))).toBe('5a2ae23');
    });

    it('returns null when metadata has no commit', () => {
      expect(getCommit(makeNode('{"confidence": 90}'))).toBeNull();
    });
  });

  describe('shortCommit', () => {
    it('returns null for null input', () => {
      expect(shortCommit(null)).toBeNull();
    });

    it('shortens long commit hash to 7 chars', () => {
      expect(shortCommit('abc123def456789')).toBe('abc123d');
    });

    it('returns full hash if already short', () => {
      expect(shortCommit('abc')).toBe('abc');
    });
  });

  describe('githubCommitUrl', () => {
    it('generates correct GitHub commit URL', () => {
      expect(githubCommitUrl('abc123', 'owner/repo'))
        .toBe('https://github.com/owner/repo/commit/abc123');
    });

    it('works with full commit hash', () => {
      expect(githubCommitUrl('5a2ae23f8b9c0d1e2f3a4b5c6d7e8f9a0b1c2d3e', 'notactuallytreyanastasio/losselot'))
        .toBe('https://github.com/notactuallytreyanastasio/losselot/commit/5a2ae23f8b9c0d1e2f3a4b5c6d7e8f9a0b1c2d3e');
    });
  });

  describe('getConfidenceLevel', () => {
    it('returns null for null confidence', () => {
      expect(getConfidenceLevel(null)).toBeNull();
    });

    it('returns high for 70+', () => {
      expect(getConfidenceLevel(70)).toBe('high');
      expect(getConfidenceLevel(100)).toBe('high');
      expect(getConfidenceLevel(85)).toBe('high');
    });

    it('returns med for 40-69', () => {
      expect(getConfidenceLevel(40)).toBe('med');
      expect(getConfidenceLevel(69)).toBe('med');
      expect(getConfidenceLevel(55)).toBe('med');
    });

    it('returns low for < 40', () => {
      expect(getConfidenceLevel(0)).toBe('low');
      expect(getConfidenceLevel(39)).toBe('low');
      expect(getConfidenceLevel(20)).toBe('low');
    });
  });

  describe('parseNode', () => {
    it('converts DecisionNode to ParsedNode with extracted metadata', () => {
      const node: DecisionNode = {
        id: 42,
        node_type: 'action',
        title: 'Implemented feature',
        description: 'Details here',
        status: 'completed',
        created_at: '2024-01-15T10:30:00Z',
        updated_at: '2024-01-15T11:00:00Z',
        metadata_json: '{"confidence": 90}',
      };

      const parsed = parseNode(node);

      expect(parsed.id).toBe(42);
      expect(parsed.node_type).toBe('action');
      expect(parsed.title).toBe('Implemented feature');
      expect(parsed.description).toBe('Details here');
      expect(parsed.status).toBe('completed');
      expect(parsed.metadata).toEqual({ confidence: 90 });
      expect(parsed.confidence).toBe(90);
    });

    it('handles node without metadata', () => {
      const node: DecisionNode = {
        id: 1,
        node_type: 'observation',
        title: 'Noticed something',
        description: null,
        status: 'pending',
        created_at: '2024-01-01T00:00:00Z',
        updated_at: '2024-01-01T00:00:00Z',
        metadata_json: null,
      };

      const parsed = parseNode(node);

      expect(parsed.metadata).toBeNull();
      expect(parsed.confidence).toBeNull();
    });
  });
});
