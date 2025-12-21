// Main graph visualization component using Cytoscape.js

import { useEffect, useRef, useState } from 'react';
import cytoscape, { Core, NodeSingular, EdgeSingular } from 'cytoscape';
import cola from 'cytoscape-cola';
import dagre from 'cytoscape-dagre';
import type { GraphData, GraphNode, GraphEdge, LayoutType, GraphDiffData } from '../types/graph';
import { getNodeColor, getEdgeColor, getDiffColor } from '../utils/colors';
import { getLayoutOptions } from '../utils/graph-layout';

// Register layout extensions
cytoscape.use(cola);
cytoscape.use(dagre);

interface GraphCanvasProps {
  graphData: GraphData | null;
  diffData: GraphDiffData | null;
  selectedNode: string | null;
  layoutType: LayoutType;
  onNodeSelect: (nodeId: string | null) => void;
  onNodeDoubleClick: (node: GraphNode) => void;
}

export const GraphCanvas = ({
  graphData,
  diffData,
  selectedNode,
  layoutType,
  onNodeSelect,
  onNodeDoubleClick,
}: GraphCanvasProps) => {
  const containerRef = useRef<HTMLDivElement>(null);
  const cyRef = useRef<Core | null>(null);
  const [isInitialized, setIsInitialized] = useState(false);

  // Initialize Cytoscape
  useEffect(() => {
    if (!containerRef.current || isInitialized) return;

    const cy = cytoscape({
      container: containerRef.current,
      style: [
        {
          selector: 'node',
          style: {
            'label': 'data(name)',
            'text-valign': 'center',
            'text-halign': 'center',
            'font-size': '12px',
            'background-color': 'data(color)',
            'width': '60px',
            'height': '60px',
            'border-width': '2px',
            'border-color': '#fff',
            'text-wrap': 'wrap',
            'text-max-width': '80px',
          },
        },
        {
          selector: 'node:selected',
          style: {
            'border-width': '4px',
            'border-color': '#3b82f6',
          },
        },
        {
          selector: 'edge',
          style: {
            'width': 2,
            'line-color': 'data(color)',
            'target-arrow-color': 'data(color)',
            'target-arrow-shape': 'triangle',
            'curve-style': 'bezier',
            'arrow-scale': 1.5,
          },
        },
        {
          selector: 'edge.recursive',
          style: {
            'curve-style': 'unbundled-bezier',
            'control-point-distances': [40],
            'control-point-weights': [0.5],
          },
        },
        {
          selector: 'edge.trait-call',
          style: {
            'line-style': 'dashed',
          },
        },
        {
          selector: '.diff-added',
          style: {
            'border-color': getDiffColor('added'),
            'border-width': '4px',
          },
        },
        {
          selector: '.diff-removed',
          style: {
            'border-color': getDiffColor('removed'),
            'border-width': '4px',
            'opacity': 0.6,
          },
        },
        {
          selector: '.diff-modified',
          style: {
            'border-color': getDiffColor('modified'),
            'border-width': '4px',
          },
        },
      ],
      wheelSensitivity: 0.2,
    });

    // Event handlers
    cy.on('tap', 'node', (event) => {
      const node = event.target;
      onNodeSelect(node.id());
    });

    cy.on('tap', (event) => {
      if (event.target === cy) {
        onNodeSelect(null);
      }
    });

    cy.on('dbltap', 'node', (event) => {
      const node = event.target;
      const nodeData = node.data() as GraphNode;
      onNodeDoubleClick(nodeData);
    });

    cyRef.current = cy;
    setIsInitialized(true);

    return () => {
      cy.destroy();
    };
  }, [isInitialized, onNodeSelect, onNodeDoubleClick]);

  // Update graph data
  useEffect(() => {
    const cy = cyRef.current;
    if (!cy || !graphData) return;

    // Clear existing elements
    cy.elements().remove();

    // Prepare node data with colors
    const nodes = graphData.nodes.map((node) => ({
      data: {
        id: node.id,
        name: node.name,
        kind: node.kind,
        color: getNodeColor(node.kind),
        ...node,
      },
    }));

    // Prepare edge data with colors
    const edges = graphData.edges.map((edge) => ({
      data: {
        id: `${edge.source}-${edge.target}`,
        source: edge.source,
        target: edge.target,
        color: getEdgeColor(edge.is_recursive, edge.is_trait_call, edge.is_generic_instantiation),
        ...edge,
      },
      classes: [
        edge.is_recursive ? 'recursive' : '',
        edge.is_trait_call ? 'trait-call' : '',
      ].filter(Boolean).join(' '),
    }));

    // Add elements to graph
    cy.add([...nodes, ...edges]);

    // Apply diff styling if available
    if (diffData) {
      // Mark added nodes
      diffData.nodes_added.forEach((node) => {
        const cyNode = cy.getElementById(node.id);
        if (cyNode.length > 0) {
          cyNode.addClass('diff-added');
        }
      });

      // Mark removed nodes
      diffData.nodes_removed.forEach((node) => {
        const cyNode = cy.getElementById(node.id);
        if (cyNode.length > 0) {
          cyNode.addClass('diff-removed');
        }
      });

      // Mark modified nodes
      diffData.nodes_modified.forEach(([_nodeA, nodeB]) => {
        const cyNode = cy.getElementById(nodeB.id);
        if (cyNode.length > 0) {
          cyNode.addClass('diff-modified');
        }
      });
    }

    // Apply layout
    const layout = cy.layout(getLayoutOptions(layoutType));
    layout.run();
  }, [graphData, diffData, layoutType]);

  // Handle selected node highlighting
  useEffect(() => {
    const cy = cyRef.current;
    if (!cy) return;

    cy.nodes().removeClass('selected');
    if (selectedNode) {
      cy.getElementById(selectedNode).addClass('selected');
    }
  }, [selectedNode]);

  return (
    <div
      ref={containerRef}
      style={{
        width: '100%',
        height: '100%',
        backgroundColor: '#1a1a1a',
      }}
    />
  );
};

