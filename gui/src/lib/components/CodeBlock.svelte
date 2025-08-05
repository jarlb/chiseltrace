<script lang="ts">
    import { onMount } from 'svelte';
    import hljs from 'highlight.js';
    import 'highlight.js/styles/github.css';

    // Make code and language accessible as properties
    export let code: string;
    export let language = 'scala';
  
    let highlightedCode = '';
  
    onMount(() => {
      highlightedCode = hljs.highlight(code, { language }).value;
    });
  </script>
  
  <div class="code-block">
    {@html highlightedCode}
  </div>
  
  <style>
    .code-block {
      font-family: 'Fira Code', monospace;
      font-size: 0.8rem;
      max-width: 400px;
      max-height: 300px;
      overflow: auto;
      background: white;
      border-radius: 4px;
      box-shadow: 0 2px 10px rgba(0,0,0,0.15);
      padding: 0.5rem;
    }
</style>