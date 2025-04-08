<script lang="ts">
  import { onMount } from 'svelte';
  import { Network } from 'vis-network/esnext';
  import { DataSet } from 'vis-data';
  import type { Edge, Node, Options } from 'vis-network/esnext';
  import 'vis-network/styles/vis-network.css';

  let networkContainer: HTMLDivElement;
  let scrollWrapper: HTMLDivElement;
  let network: Network;

  interface Timestamp {
    id: string;
    time: string;
    xPos: number;
    width: number;
  }

  interface CustomNode extends Node {
    group: string;
  }

  const timestamps: Timestamp[] = [
    { id: 't1', time: '00:00', xPos: 0, width: 600 },
    { id: 't2', time: '00:05', xPos: 600, width: 600 },
    { id: 't3', time: '00:10', xPos: 1200, width: 600 },
    { id: 't4', time: '00:15', xPos: 1800, width: 600 },
    { id: 't5', time: '00:20', xPos: 2400, width: 600 },
    { id: 't6', time: '00:25', xPos: 3000, width: 600 },
    { id: 't7', time: '00:30', xPos: 3600, width: 600 },
    { id: 't8', time: '00:35', xPos: 4200, width: 600 }
  ];

  const nodes = new DataSet<CustomNode>([
  // Original nodes
  { id: 1, label: 'Event A', group: 't1' },
  { id: 2, label: 'Event B', group: 't1' },
  { id: 3, label: 'Event C', group: 't1' },
  { id: 4, label: 'Event D', group: 't2' },
  { id: 5, label: 'Event E', group: 't2' },
  { id: 6, label: 'Event F', group: 't3' },
  
  // Added nodes
  { id: 7, label: 'Event G', group: 't1' },
  { id: 8, label: 'Event H', group: 't2' },
  { id: 9, label: 'Event I', group: 't2' },
  { id: 10, label: 'Event J', group: 't3' },
  { id: 11, label: 'Event K', group: 't3' },
  { id: 12, label: 'Event L', group: 't4' },
  { id: 13, label: 'Event M', group: 't4' },
  { id: 14, label: 'Event N', group: 't5' },
  { id: 15, label: 'Event O', group: 't5' },
  { id: 16, label: 'Event P', group: 't6' },
  { id: 17, label: 'Event Q', group: 't6' },
  { id: 18, label: 'Event R', group: 't7' },
  { id: 19, label: 'Event S', group: 't7' },
  { id: 20, label: 'Event T', group: 't8' }
]);

const edges = new DataSet<Edge>([
  // Original edges
  { from: 1, to: 2, arrows: 'to' },
  { from: 1, to: 3, arrows: 'to' },
  { from: 2, to: 4, arrows: 'to' },
  { from: 3, to: 5, arrows: 'to' },
  { from: 4, to: 6, arrows: 'to' },
  { from: 5, to: 6, arrows: 'to' },
  
  // Added edges
  { from: 1, to: 7, arrows: 'to' },
  { from: 7, to: 8, arrows: 'to' },
  { from: 7, to: 9, arrows: 'to' },
  { from: 8, to: 10, arrows: 'to' },
  { from: 9, to: 11, arrows: 'to' },
  { from: 10, to: 12, arrows: 'to' },
  { from: 11, to: 13, arrows: 'to' },
  { from: 6, to: 14, arrows: 'to' },
  { from: 12, to: 14, arrows: 'to' },
  { from: 13, to: 15, arrows: 'to' },
  { from: 14, to: 16, arrows: 'to' },
  { from: 15, to: 17, arrows: 'to' },
  { from: 16, to: 18, arrows: 'to' },
  { from: 17, to: 19, arrows: 'to' },
  { from: 18, to: 20, arrows: 'to' },
  { from: 19, to: 20, arrows: 'to' },
  { from: 3, to: 8, arrows: 'to' },
  { from: 4, to: 11, arrows: 'to' },
  { from: 5, to: 10, arrows: 'to' },
  { from: 9, to: 14, arrows: 'to' },
  { from: 10, to: 15, arrows: 'to' },
  { from: 12, to: 16, arrows: 'to' }
]);

  onMount(() => {
    network = new Network(networkContainer, { nodes, edges }, getNetworkOptions());

    // Add resize observer
    const resizeObserver = new ResizeObserver(() => {
      console.log("resize");
      network.setSize(`${networkContainer.clientWidth}px`, `${networkContainer.clientHeight}px`);
    });
    resizeObserver.observe(networkContainer);

    network.once("stabilizationIterationsDone", () => {
      // Add a small delay to reposition the graph to be correct with the time line
      setTimeout(() => {
        // network.setOptions({ physics: { enabled: false } });
        network.moveTo({
          position: { x: scrollWrapper.clientWidth/2, y: getCenteredNetworkY() },
          scale: 1,
          animation: false
        });
      }, 1);

      setTimeout(() => {
        network.setOptions({ physics: { enabled: false } });
      }, 1000);  
    });

    network.on("beforeDrawing", () => {
      const nodePositions = network.getPositions();
      nodes.forEach((node) => {
        const nodeTimestamp = timestamps.find(t => t.id === node.group);
        if (nodeTimestamp) {
          const currentPos = nodePositions[node.id!];
          const constrainedX = Math.max(
            nodeTimestamp.xPos + 50,
            Math.min(nodeTimestamp.xPos + nodeTimestamp.width - 50, currentPos.x)
          );
          if (Math.abs(currentPos.x - constrainedX) > 0.1) {
            network.moveNode(node.id!, constrainedX, currentPos.y);
          }
        }
      });
    });

    scrollWrapper.addEventListener('wheel', handleScroll);

    return () => {
      resizeObserver.unobserve(networkContainer);
      network.off('dragEnd');
      scrollWrapper.removeEventListener('wheel', handleScroll);
    };
  });

  function getNetworkOptions(): Options {
    return {
      interaction: {
        zoomView: false,
        dragView: false,
        dragNodes: true
      },
      physics: {
        enabled: true,
        solver: 'barnesHut', // Use Barnes-Hut approximation
        stabilization: {
          enabled: true,
          iterations: 10000,
          updateInterval: 1
        },
        barnesHut: {
          gravitationalConstant: -2000,  // Overall repulsion strength
          centralGravity: 0.3,         // Pull toward center
          springLength: 150,           // Ideal edge length
          springConstant: 0.001,        // Edge attraction strength (lower = weaker)
          damping: 0.09,               // Friction
          avoidOverlap: 0.1            // Node spacing
        }
      },
      nodes: {
        shape: 'box',
        margin: { top: 8, bottom: 8, left: 8, right: 8 },
        widthConstraint: { maximum: 180 },
        font: { size: 14 },
        color: {
          border: '#2B6CB0',
          background: '#EBF8FF',
          highlight: {
            border: '#2B6CB0',
            background: '#BEE3F8'
          }
        }
      },
      edges: {
        smooth: { enabled: true, type: 'continuous', roundness: 0.4 },
        arrows: { to: { scaleFactor: 0.6 } },
        color: { color: '#718096', highlight: '#4A5568' }
      },
      groups: {
        t1: { color: { background: '#EBF8FF' } },
        t2: { color: { background: '#EBF8FF' } },
        t3: { color: { background: '#EBF8FF' } }
      }
    };
  }

  let timelinePosition = 0;

  function handleScroll(event: WheelEvent) {
    timelinePosition = Math.min(timestamps.length * 600 - scrollWrapper.clientWidth, Math.max(0, timelinePosition + event.deltaY));
    console.log(timelinePosition);
    // Update timeline positions
    document.querySelectorAll<HTMLElement>('.timeslot-header, .timeslot-line').forEach(el => {
      el.style.transform = `translateX(-${timelinePosition}px)`;
    });
    // Update network viewport

    network.moveTo({
      position: { x: timelinePosition + scrollWrapper.clientWidth/2, y: getCenteredNetworkY() },
      animation: false
    });
  }

  function getTimeslotsInView(): string[] {
    let slotsInView: string[] = [];
    let pixelCounter = 0;
    timestamps.forEach((timestamp) => {
      pixelCounter += timestamp.width;
      if (pixelCounter > timelinePosition && pixelCounter - timestamp.width < timelinePosition + scrollWrapper.clientWidth) {
        slotsInView.push(timestamp.id);
      }
    });
    return slotsInView;
  }

  function getCenteredNetworkY(): number {
    const timeslotsInView = getTimeslotsInView();
    const nodePositions = network.getPositions();
    let yAcc = 0;
    let nAcc = 0;
    nodes.forEach((node) => {
      const currentPos = nodePositions[node.id!];
      if (timeslotsInView.includes(node.group)) {
        yAcc += currentPos.y;
        nAcc ++;
      }
    });
    return yAcc / nAcc;
  }
</script>

<div class="graph-container">
  <div class="scroll-wrapper" bind:this={scrollWrapper}>
    <div class="timeline-header">
      {#each timestamps as timestamp}
        <div class="timeslot-header" style="left: {timestamp.xPos}px;">
          {timestamp.time}
        </div>
      {/each}
    </div>
    
    <div class="timeslot-grid">
      {#each timestamps as timestamp}
        <div class="timeslot-line" style="left: {timestamp.xPos}px;"></div>
      {/each}
    </div>

    <div class="network-wrapper">
      <div bind:this={networkContainer} class="network"></div>
    </div>
  </div>
</div>

<style>
  :global(html, body) {
    height: 100%;
    margin: 0;
    padding: 0;
    overflow: hidden;
  }

  .graph-container {
    height: 100vh;
    overflow: hidden;
    background-color: #F7FAFC;
    border: 1px solid #E2E8F0;
    border-radius: 6px;
  }

  .scroll-wrapper {
    height: 100%;
  }

  .timeline-header {
    position: sticky;
    top: 0;
    height: 40px;
    background: white;
    z-index: 100;
    border-bottom: 1px solid #E2E8F0;
  }

  .timeslot-grid {
    position: absolute;
    top: 40px;
    bottom: 0;
    width: 100%;
    pointer-events: none;
    z-index: 50;
  }

  .timeslot-line {
    position: absolute;
    width: 1px;
    background: rgba(66, 153, 225, 0.2);
    height: 100%;
  }

  .network-wrapper {
    height: calc(100vh - 40px);
    width: 100vw;
    position: fixed;
  }

  .network {
    height: 100%;
    background: transparent;
  }

  .timeslot-header {
    position: absolute;
    top: 8px;
    padding: 4px;
    background: #4299E1;
    color: white;
    border-radius: 20px;
    font-size: 14px;
    font-weight: 500;
    box-shadow: 0 1px 3px rgba(0, 0, 0, 0.1);
    z-index: 100;
  }
</style>