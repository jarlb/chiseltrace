<script lang="ts">
  import { onMount } from 'svelte';
  import { invoke } from "@tauri-apps/api/core";
  import { Network } from 'vis-network/esnext';
  import { DataSet } from 'vis-data';
  import type { Edge, Node, Options, Position } from 'vis-network/esnext';
  import 'vis-network/styles/vis-network.css';

  import CodeBlock from '../../lib/components/CodeBlock.svelte';

  let networkContainer: HTMLDivElement;
  let scrollWrapper: HTMLDivElement;
  let network: Network;

  let timestampsInGraph: number[] = [];

  let hoveredNode: any = null;
  let tooltipPosition = { x: 0, y: 0 };

  let showMenu = false;
  let menuX = 0;
  let menuY = 0;
  let contextMenuNode: CustomNode | null = null;
  let contextNodeTarget: boolean = false;

  interface Timestamp {
    id: string;
    time: string;
    xPos: number;
    width: number;
  }

  interface Signal {
    name: string,
    value: string,
    connectionType: string
  }

  interface CustomNode extends Node {
    group: string;
    modulePath: string[];
    timestamp: number;
    longDistance: boolean;
    code: string | null;
    incoming: Signal[];
    outgoing: Signal[];
    file: string,
    line: number
  }

  interface ViewerGraph {
    vertices: CustomNode[];
    edges: Edge[];
  }

  let timestamps: Timestamp[] = [];
  const nodes = new DataSet<CustomNode>([]);
  const edges = new DataSet<Edge>([]);
  let positionCache = new Map<string, Position>();

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

  async function updateGraph(reset_graph: boolean = false) {
    const timestampsToLoad = getTimestampsToLoad();
    if (!arraysEqual(timestampsToLoad, timestampsInGraph) || reset_graph) {
      let removedTimestamps = timestampsInGraph.filter(item => !timestampsToLoad.includes(item));
      let newTimestamps = timestampsToLoad.filter(item => !timestampsInGraph.includes(item));
      if (reset_graph) {
        removedTimestamps = timestampsToLoad;
        newTimestamps = timestampsToLoad;
      }
      const response = await invoke<string>("get_partial_graph", {rangeBegin: Math.min(...timestampsToLoad), rangeEnd: Math.max(...timestampsToLoad)});
      try {
        const g: ViewerGraph = JSON.parse(response);
        const nodesToRemove = nodes.get({
          filter: (node) => removedTimestamps.includes(node.timestamp)
        });
        // save the old node positions
        let oldPositions = network.getPositions(nodesToRemove.map(node => node.id));
        Object.entries(oldPositions).forEach(([id, pos]) => {
          positionCache.set(id, pos);
        });
        console.log(positionCache);
        nodes.remove(nodesToRemove.map(node => node.id));

        const nodesToAdd = g.vertices.flatMap(node => {
          if (newTimestamps.includes(node.timestamp)) {
            const oldPos = positionCache.get(node.id!.toString());
            if (oldPos !== undefined) {
              // Node has a saved position -> set the position and disable placement
              return [{
                ...node,
                x: oldPos.x,
                y: oldPos.y,
                physics: false,
                fixed: {
                  x: false,
                  y: false
                }
              }];
            } else {
              // New node without saved position -> automatic placement
              return [{
                ...node,
                physics: true
              }];
            }
          } else {
            return [];
          }
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

  async function toggleModule() {
    if (contextMenuNode !== null) {
      await invoke("toggle_module", {modulePath: contextMenuNode.modulePath, timestamp: contextMenuNode.timestamp});
      await updateGraph(true);
      showMenu = false;
    }
  }

  async function setNewHead() {
    if (contextMenuNode !== null) {
      await invoke("set_new_head", {id: contextMenuNode.id});
      await updateGraph(true);
      showMenu = false;
    }
  }

  async function openIde() {
    if (contextMenuNode !== null) {
      await invoke("open_vs_code", {id: contextMenuNode.id});
      showMenu = false;
    }
  }

  async function resetGraph() {
    await invoke("reset_head", {});
    await updateGraph(true);
    showMenu = false;
  }

  // Turn off the physics for all nodes in view.
  function freezeAllNodes() {
    const updates = nodes.getIds().map(id => {
      const node = nodes.get(id) as CustomNode;
      return {
        id,
        physics: false,
        color: node.color
      };
    });
  
    nodes.update(updates);
    network.redraw(); // Force immediate render
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

      // Add the tooltip callback
      network.on("hoverNode", (event) => {
        console.log(event);
        hoveredNode = nodes.get(event.node);
        const pos = network.getPositions([event.node])[event.node];
        tooltipPosition = network.canvasToDOM(pos);
      });

      // Right click callback
      network.on("oncontext", function (params) {
          params.event.preventDefault();
          showMenu = true;
          menuX = params.event.pageX;
          menuY = params.event.pageY;
          contextNodeTarget = false;
          let nodeID = network.getNodeAt(params.pointer.DOM);
          if (nodeID !== undefined && nodeID !== null && !Array.isArray(nodeID)) {
            contextNodeTarget = true;
            let node = nodes.get(nodeID);
            contextMenuNode = node;
            console.log('Single node found:', contextMenuNode);
          }

      });

      network.on("blurNode", () => {
        hoveredNode = null;
      });

      network.on("dragStart", () => { hoveredNode = null; });

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
            let constrainedY = 0;
            if (node.longDistance) {
              constrainedY = Math.max(
                20, 
                Math.min(200, currentPos.y)
              );
            } else {
              constrainedY = Math.max(
                // I tried doing it with a % of the networkContainer.clientHeight
                // For some reason, it *really* doesn't like that and pegs the CPU at 100% ¯\_(ツ)_/¯
                250, 
                Math.min(networkContainer.clientHeight - 50, currentPos.y)
              );
            }
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
      network.off('hoverNode');
      network.off('blurNode');
      scrollWrapper.removeEventListener('wheel', handleScroll);
      window.removeEventListener('resize', handleWindowResize)
    };
  });

  function getNetworkOptions(): Options {
    return {
      interaction: {
        zoomView: false,
        dragView: false,
        dragNodes: true,
        hover: true
      },
      physics: {
        enabled: true,
        solver: 'barnesHut',
        stabilization: {
          enabled: true,
          iterations: 10,
          updateInterval: 1,
          fit: false
        },
        barnesHut: { // These really need some tweaking for a production app
          gravitationalConstant: -1000, // Overall repulsion strength
          centralGravity: 0.0, // Pull toward center
          springLength: 150, // Ideal edge length
          springConstant: 0.005, // Edge attraction strength (lower = weaker)
          damping: 0.09, // Friction
          avoidOverlap: 1 // Node spacing
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
    }, 50); // This is done to not overload the JS engine while the user scrolls, only update at the end of the scroll

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
  // No longer used, this is done in the back-end
  // function getTimeslotsInView(): string[] {
  //   let slotsInView: string[] = [];
  //   let pixelCounter = 0;
  //   timestamps.forEach((timestamp) => {
  //     pixelCounter += timestamp.width;
  //     if (pixelCounter > timelinePosition && pixelCounter - timestamp.width < timelinePosition + scrollWrapper.clientWidth) {
  //       slotsInView.push(timestamp.id);
  //     }
  //   });
  //   console.log(slotsInView);
  //   return slotsInView;
  // }

  // This function is responsible for determining the time stamps that should be loaded
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

  function handleWindowResize() {
    network.moveTo({
      position: { x: timelinePosition + scrollWrapper.clientWidth/2, y: scrollWrapper.clientHeight/2 },
      scale: 1,
      animation: false
    });
  }

  function closeMenu() {
    showMenu = false;
  }
</script>

{#if showMenu}
  <div
    class="context-menu"
    style={`left: ${menuX}px; top: ${menuY}px`}
    on:click|stopPropagation
  >
    {#if contextNodeTarget}
      <div class="menu-item" on:click={async () => toggleModule()}>Toggle module</div>
      <div class="menu-item" on:click={async () => setNewHead()}>Make new head</div>
      <div class="menu-item" on:click={async () => openIde()}>Show in VS Code</div>
    {:else}
      <div class="menu-item" on:click={async () => resetGraph()}>Reset graph</div>
    {/if}
    <div class="menu-item" on:click={closeMenu}>Close</div>
  </div>
{/if}

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
      {#if hoveredNode}
      <div class="node-tooltip" style={`left: ${tooltipPosition.x}px; top: ${tooltipPosition.y}px`}>
        <h3>{hoveredNode.label}</h3>
        <p style="max-width: 100%; word-break: break-all;">{hoveredNode.file}:{hoveredNode.line}</p>
        {#if hoveredNode.code}
          <CodeBlock code={hoveredNode.code}></CodeBlock>
        {/if}

        <div class="signals-container">
          {#if hoveredNode.incoming?.length}
            <div class="signal-list incoming">
              <h4>Incoming Signals</h4>
              {#each hoveredNode.incoming as signal}
                <div class="signal-item">
                  <span class="signal-name">{signal.name}</span>
                  <span class="signal-value">{signal.value ?? 'null'}</span>
                  <span class="signal-connection {signal.connectionType}">
                    {signal.connectionType}
                  </span>
                </div>
              {/each}
            </div>
          {/if}
      
          {#if hoveredNode.outgoing?.length}
            <div class="signal-list outgoing">
              <h4>Outgoing Signals</h4>
              {#each hoveredNode.outgoing as signal}
                <div class="signal-item">
                  <span class="signal-name">{signal.name}</span>
                  <span class="signal-value">{signal.value ?? 'null'}</span>
                  <span class="signal-connection {signal.connectionType}">
                    {signal.connectionType}
                  </span>
                </div>
              {/each}
            </div>
          {/if}
        </div>

      </div>
    {/if}
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

  .context-menu {
    position: fixed;
    background: white;
    border: 1px solid #ccc;
    border-radius: 4px;
    box-shadow: 0 2px 10px rgba(0,0,0,0.1);
    z-index: 1000;
    min-width: 120px;
  }

  .menu-item {
    padding: 8px 12px;
    cursor: pointer;
  }

  .menu-item:hover {
    background-color: #f0f0f0;
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
    background-color: rgba(66, 153, 225, 0.2);
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

  .node-tooltip {
    position: absolute;
    background: white;
    border: 1px solid #ddd;
    border-radius: 4px;
    padding: 8px 12px;
    box-shadow: 0 2px 8px rgba(0,0,0,0.1);
    z-index: 1000;
    pointer-events: none;
    transform: translate(10px, -50%);
    max-width: 400px;
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

  .signals-container {
    margin-top: 8px;
  }

  .signal-list h4 {
    margin: 8px 0 4px 0;
    font-size: 12px;
    color: #586069;
    font-weight: 600;
  }

  .signal-item {
    display: grid;
    grid-template-columns: 1.5fr 1fr 0.8fr;
    gap: 8px;
    align-items: center;
    padding: 4px 0;
    border-bottom: 1px solid #f0f0f0;
  }

  .signal-name {
    font-weight: 500;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .signal-value {
    font-family: monospace;
    font-size: 12px;
    color: #666;
    text-align: right;
    padding-right: 4px;
  }

  .signal-connection {
    font-size: 11px;
    text-align: center;
    padding: 2px 4px;
    border-radius: 3px;
  }

  /* Connection type styling */
  .signal-connection.data {
    background: #d4e6fd;
    color: #62a6ff;
  }
  .signal-connection.index {
    background: #fbdaff;
    color: #c153ce;
  }
  .signal-connection.controlflow {
    background: #ffd7d8;
    color: #ff6568;
  }
</style>