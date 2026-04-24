<script lang="ts">
  let { letter, color }: { letter: string; color: string } = $props();
</script>

<span class="space-badge" style="background: {color}">
  {letter}
</span>

<style>
  .space-badge {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    width: 18px;
    height: 18px;
    border-radius: 50%;
    font-size: 10px;
    font-weight: 600;
    color: white;
    flex-shrink: 0;
    line-height: 1;
  }
</style>
