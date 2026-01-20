// Graph layout configuration for Cytoscape

import type { LayoutType } from '../types/graph';

export const getLayoutOptions = (layoutType: LayoutType) => {
  switch (layoutType) {
    case 'cola':
      return {
        name: 'cola',
        animate: true,
        maxSimulationTime: 4000,
        ungrabifyWhileSimulating: false,
        fit: true,
        padding: 30,
        nodeDimensionsIncludeLabels: true,
        randomize: false,
        avoidOverlap: true,
        handleDisconnected: true,
        convergenceThreshold: 0.01,
        nodeSpacing: 100,
      };

    case 'dagre':
      return {
        name: 'dagre',
        rankDir: 'TB', // top to bottom
        align: 'UL',
        ranker: 'network-simplex',
        nodeSep: 50,
        rankSep: 100,
        padding: 30,
        fit: true,
        animate: false,
      };

    case 'circle':
      return {
        name: 'circle',
        fit: true,
        padding: 30,
        animate: true,
        animationDuration: 500,
        avoidOverlap: true,
        radius: undefined,
        startAngle: (3 / 2) * Math.PI,
        sweep: undefined,
        clockwise: true,
        sort: undefined,
      };

    case 'grid':
      return {
        name: 'grid',
        fit: true,
        padding: 30,
        animate: true,
        animationDuration: 500,
        avoidOverlap: true,
        avoidOverlapPadding: 10,
        condense: false,
        rows: undefined,
        cols: undefined,
        position: undefined,
        sort: undefined,
      };

    default:
      return getLayoutOptions('cola');
  }
};

