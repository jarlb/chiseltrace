<script lang="ts">
  import { onMount } from 'svelte';
  import { invoke } from "@tauri-apps/api/core";
  import { Network } from 'vis-network/esnext';
  import { DataSet } from 'vis-data';
  import type { Edge, Node, Options } from 'vis-network/esnext';
  import 'vis-network/styles/vis-network.css';

  let networkContainer: HTMLDivElement;
  let scrollWrapper: HTMLDivElement;
  let network: Network;

  let timestampsInGraph: number[] = [];

  interface Timestamp {
    id: string;
    time: string;
    xPos: number;
    width: number;
  }

  interface CustomNode extends Node {
    group: string;
    timestamp: number;
  }

  interface ViewerGraph {
    vertices: CustomNode[];
    edges: Edge[];
  }

  let timestamps: Timestamp[] = [];
  const nodes = new DataSet<CustomNode>([]);
  const edges = new DataSet<Edge>([]);

  function generateReverseIndexTimestamps(num_timestamps: number): Timestamp[] {
    const timestamps: Timestamp[] = [];
    
    for (let i = 0; i <= num_timestamps; i++) {
      const reverseIndex = num_timestamps - i;
      timestamps.push({
        id: `t${i}`,
        time: reverseIndex.toString(),
        xPos: i * 600,
        width: 600
      });
    }
    
    return timestamps;
  }

  function arraysEqual(a: number[], b: number[]): boolean {
    return a.length === b.length && a.every((value, index) => value === b[index]);
  }

  async function updateGraph() {
    const timestampsToLoad = getTimestampsToLoad();
    if (!arraysEqual(timestampsToLoad, timestampsInGraph)) {
      const removedTimestamps = timestampsInGraph.filter(item => !timestampsToLoad.includes(item));
      const newTimestamps = timestampsToLoad.filter(item => !timestampsInGraph.includes(item));
      const response = await invoke<string>("get_partial_graph", {rangeBegin: Math.min(...timestampsToLoad), rangeEnd: Math.max(...timestampsToLoad)});
      try {
        const g: ViewerGraph = JSON.parse(response);
        const nodesToRemove = nodes.get({
          filter: (node) => removedTimestamps.includes(node.timestamp)
        });
        nodes.remove(nodesToRemove.map(node => node.id));

        const nodesToAdd = g.vertices.flatMap(node => {
          if (newTimestamps.includes(node.timestamp)) {
            return [node];
          } else { return []; }
        });
        console.log(nodesToRemove);
        console.log(nodesToAdd);
        nodes.add(nodesToAdd);
        edges.clear();
        edges.add(g.edges);
        network.stabilize();
        setTimeout(freezeAllNodes, 1000);

      } catch (error) {
        console.error("Failed to parse response", error);
      }
      timestampsInGraph = timestampsToLoad;
    }
  }

  // Turn off the physics for all nodes in view.
  function freezeAllNodes() {    
    const updates = nodes.getIds().map(id => ({
      id,
      physics: false
    }));
    
    nodes.update(updates);
  }

  onMount(() => {
    invoke<number>("get_n_timeslots").then((num_timestamps) => {
      timestamps = generateReverseIndexTimestamps(num_timestamps);

      updateGraph().catch(error => {
        console.error(error);
      });

      network = new Network(networkContainer, { nodes, edges }, getNetworkOptions());

      network.once("stabilizationIterationsDone", () => {
        // Add a small delay to reposition the graph to be correct with the time line
        setTimeout(() => {
          network.moveTo({
            position: { x: scrollWrapper.clientWidth/2, y: scrollWrapper.clientHeight/2 },
            scale: 1,
            animation: false
          });
        }, 1);

        setTimeout(() => {
          freezeAllNodes();
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
            const constrainedY = Math.max(
              // I tried doing it with a % of the networkContainer.clientHeight
              // For some reason, it *really* doesn't like that and pegs the CPU at 100%
              250, 
              Math.min(networkContainer.clientHeight - 50, currentPos.y)
            );
            if (Math.abs(currentPos.x - constrainedX) > 0.1 || Math.abs(currentPos.y - constrainedY) > 0.1) {
              network.moveNode(node.id!, constrainedX, constrainedY);
            }
          }
        });
      });
    });

    // Add resize observer
    const resizeObserver = new ResizeObserver(() => {
      console.log("resize");
      network.setSize(`${networkContainer.clientWidth}px`, `${networkContainer.clientHeight}px`);
    });
    resizeObserver.observe(networkContainer);

    scrollWrapper.addEventListener('wheel', handleScroll);

    window.addEventListener('resize', handleWindowResize);

    return () => {
      resizeObserver.unobserve(networkContainer);
      network.off('dragEnd');
      scrollWrapper.removeEventListener('wheel', handleScroll);
      window.removeEventListener('resize', handleWindowResize)
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
          iterations: 10,
          updateInterval: 1,
          fit: false
        },
        barnesHut: {
          gravitationalConstant: -1000,  // Overall repulsion strength
          centralGravity: 0.0,         // Pull toward center
          springLength: 150,           // Ideal edge length
          springConstant: 0.005,        // Edge attraction strength (lower = weaker)
          damping: 0.09,               // Friction
          avoidOverlap: 1            // Node spacing
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
  let debounceTimer: number | null = null;
  async function handleScroll(event: WheelEvent) {
    if (debounceTimer) {
      clearTimeout(debounceTimer);
    }
    debounceTimer = setTimeout(async () => {
      await updateGraph();
    }, 50); // Adjust timing as needed (milliseconds)

    timelinePosition = Math.min(timestamps.length * 600 - scrollWrapper.clientWidth, Math.max(0, timelinePosition + event.deltaY));
    console.log(timelinePosition);
    // Update timeline positions
    document.querySelectorAll<HTMLElement>('.timeslot-header, .timeslot-line').forEach(el => {
      el.style.transform = `translateX(-${timelinePosition}px)`;
    });
    // Update network viewport
    network.moveTo({
      position: { x: timelinePosition + scrollWrapper.clientWidth/2, y: scrollWrapper.clientHeight/2 },
      scale: 1,
      animation: false
    });
  }

  // Gets the ID's of the timeslots in view. Used to center the graph properly
  function getTimeslotsInView(): string[] {
    let slotsInView: string[] = [];
    let pixelCounter = 0;
    timestamps.forEach((timestamp) => {
      pixelCounter += timestamp.width;
      if (pixelCounter > timelinePosition && pixelCounter - timestamp.width < timelinePosition + scrollWrapper.clientWidth) {
        slotsInView.push(timestamp.id);
      }
    });
    console.log(slotsInView);
    return slotsInView;
  }

  // This function is responsible for determining the time stamps that should be loaded
  // =========================================================================
  function getTimestampsToLoad(offset: number = 1800): number[] {
    let timeStampsInRange: number[] = [];
    let pixelCounter = 0;
    timestamps.forEach((timestamp) => {
      pixelCounter += timestamp.width;
      if (pixelCounter > timelinePosition - offset && pixelCounter - timestamp.width < timelinePosition + scrollWrapper.clientWidth + offset) {
        timeStampsInRange.push(+timestamp.time);
      }
    });
    return timeStampsInRange;
  }
  // =========================================================================

  function handleWindowResize() {
    network.moveTo({
      position: { x: timelinePosition + scrollWrapper.clientWidth/2, y: scrollWrapper.clientHeight/2 },
      scale: 1,
      animation: false
    });
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

  .timeslot-grid::after {
    content: "";
    position: absolute;
    top: 200px;
    left: 0;
    right: 0;
    height: 1px;
    background-color: rgba(66, 153, 225, 0.2); /* Change color as needed */
    z-index: 51;
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