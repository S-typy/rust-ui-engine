# Gallery

`Controls::gallery` displays grouped visual samples. Each sample has a stable
key, a caption, an optional icon or registered image resource, additional readonly
text columns, and an enabled flag. `GalleryModel` accepts at most 256 samples and
32 groups; a `Controls` instance owns at most 32 galleries. Metadata is limited to
64 KiB, individual captions to 4096 UTF-8 bytes, and identifiers to 128 ASCII
letters, digits, `_`, `-`, `.`, or `/`.

`GallerySettings` specifies uniform sample width and height (32–1024 logical
pixels), gap (0–64), group header height (16–128), and read-only selection.
Rows adapt to the available width, groups start a new row, and a scroll viewer
contains overflow. All bounded samples remain mounted. This profile does not
provide a virtualized data-card collection, commands on samples, or rearrangement.

## Preview and acceptance

`gallery_preview_key` is transient; `gallery_selected_key` is accepted state.
Hover and arrow navigation preview an enabled sample without accepting it.
Enter, Space, a pointer release on the pressed sample, or accessibility selection
accepts it. Key repeats do not repeat acceptance. Escape, pointer leave, loss of
availability, and popup close cancel preview. Read-only galleries still permit
browsing and preview; user acceptance is disabled. Application setters remain
available.

`drain_gallery_previews` coalesces hover changes to the latest preview per gallery.
`drain_gallery_selection_changes` reports each changed user acceptance once, with
a bounded queue of 256 pending changes. Programmatic setters do not enqueue user
events. Applications display their preview using the transient key and return to
the accepted key when it becomes `None`; Gallery does not mutate application
content while previewing.

A `SelectionValidator` and binding source validators run before accepted state
changes. Unknown or disabled keys are rejected. A rejected user action remains
nonfatal and exposes `gallery_error` through the owner's accessibility validation
message. Programmatic calls return the error. A source replacement cancels stale
preview/pointer state and keeps surviving keys and their `WidgetId`s. Removing or
disabling the accepted key proposes `None` to the same validators before changing
the source. Reordering never transfers a widget identity to another key.

## Retained hosts

`set_gallery_host(owner, parent, style)` transfers the same gallery subtree to an
ordinary container or an existing `RibbonContent` host. The target must be live,
attached, enabled, visible, and outside the gallery subtree. The scroll owner's
internal overlay layout is preserved. Invalid targets and styles are rejected
before a popup closes or the tree changes.

`configure_gallery_popup(owner, anchor, size)` creates one retained `Flyout`.
`open_gallery_popup` moves the gallery into that surface and reveals the accepted
sample. `close_gallery_popup`, Escape, outside dismissal, and an unavailable
anchor return it to the current home host and cancel preview. A subsequent
`set_gallery_host` changes that home. Removing a home or popup anchor removes the
associated gallery and popup; removing the gallery removes its owned popup.
Required presentation parts cannot be individually removed through `Controls`.
External reparenting of owned parts is diagnosed before further transitions.

Closing clears focus in the departing popup before the common popup lifecycle
restores a surviving anchor. Resizing and keyboard navigation reveal the current
sample after the new layout has been arranged. Images and geometry use the
existing logical-pixel and renderer scale contracts.

## Presentation and bindings

`set_gallery_item_template` accepts the existing readonly `ListItemTemplate`
profile: stack, text, icon, and separator nodes, up to 32 nodes and eight levels.
`Text`, `Key`, and additional `Column0`–`Column30` project the current sample.
Images and icons supplied by the sample occupy the area above its caption
presentation. Interactive children, event handlers, and arbitrary runtime
objects are outside this profile. Template changes retain the sample's behavior
owner and accepted key. Candidate text and image resources are checked before
publication; bounded staging can reject a replacement if the temporary tree
cannot fit available resource limits.

`BindingProperty::SelectedKey` uses a string or `DataValue::Null` and publishes
accepted state only. `IsReadOnly` is a Boolean endpoint. Source callbacks must obey
the same non-reentrant validation contract as other controls. An invalid initial
compiled binding with no successful fallback rejects the form mount; a later
invalid source update preserves the last valid selection and records a binding
diagnostic.

```xml
<Gallery SelectedKey="{Binding SelectedSample}" SampleWidth="112"
         SampleHeight="96" Height="280">
  <Gallery.ItemTemplate>
    <TextBlock Text="{Binding Text}" TextWrapping="Wrap"/>
  </Gallery.ItemTemplate>
  <GalleryGroup Key="colors" Header="Colors">
    <GalleryItem Key="blue" Text="Blue" ImageResource="BlueSample"/>
    <GalleryItem Key="ready" Text="Ready" Icon="Check"/>
  </GalleryGroup>
</Gallery>
```

`ImageResource` is an application-registered resource name, never a path or URL.
The application registers its bytes before mounting. `GalleryColumn Text="…"`
adds readonly text data beneath a `GalleryItem`. `ItemTemplate` can also reference
a scoped `DataTemplate` resource. Only `SelectedKey`, `IsReadOnly`, common layout,
visibility, enabled, and badge properties accept ordinary bindings; item metadata
is literal. `set_gallery_model` is the Rust API for replacing a bounded source.

The example `GalleryWindow.xaml` demonstrates one gallery moving between a form,
Ribbon, and popup while preserving a separate editor draft. Automated checks cover
preview/acceptance separation, source veto, stable identities, real Ribbon/Flyout
transfers, popup owner loss, resize reveal, readonly presentation, image resources,
and compiled binding rollback. Native input and screen-reader results are recorded
separately from these deterministic checks.
