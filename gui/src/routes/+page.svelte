<script lang="ts">
    import { invoke } from "@tauri-apps/api/core";
    import { onMount } from "svelte";
    import { goto } from "$app/navigation";

    onMount(async () => {
        // This allows rust to determine the landing page. This is done to maybe add a standalone GUI in the future
        // that does not depend on command line input arguments. If no inputs are given, it could route to another page.
        // Another use for this might be to go to a loading screen if the backend detects that processing the graph is taking a long time.
        const initialRoute: string = await invoke("get_initial_route");
        goto(initialRoute);
    })
</script>