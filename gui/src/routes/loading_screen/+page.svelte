<script lang="ts">
    import { invoke } from "@tauri-apps/api/core";
    import { onMount } from "svelte";
    import { goto } from "$app/navigation";

    let headerText = "Loading graph, please wait";
    let errorMessage = "";

    onMount(async () => {
        try {
            await invoke("make_dpdg");
            goto("/view_graph");
        } catch (e) {
            headerText = "An error occurred while creating the DPDG";
            if (e instanceof Error) {
                console.log(e.message);
                errorMessage = e.message;
            } else if (typeof e === 'string') {
                console.log(e);
                errorMessage = e;
            } else {
                console.log(e);
                errorMessage = "See console for error";
            }
        }
    })
</script>

<h1>{headerText}</h1>
<p>{errorMessage}</p>