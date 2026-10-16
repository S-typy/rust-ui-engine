use crate::*;

fn source(body: &str) -> String {
    format!(
        r#"<Window xmlns="{UI_NAMESPACE}" xmlns:x="http://schemas.microsoft.com/winfx/2006/xaml">{body}</Window>"#
    )
}

#[test]
fn reusable_item_templates_compile_only_item_local_readonly_bindings() {
    let xml = source(
        r#"<Window.Resources><DataTemplate x:Key="Item">
        <StackPanel Orientation="Horizontal" Spacing="6"><Icon Symbol="Copy" Width="16"/><TextBlock Text="{Binding Text}" FontWeight="600"/><TextBlock Text="{Binding Key}"/></StackPanel>
        </DataTemplate></Window.Resources><StackPanel>
        <ListBox SelectionMode="Multiple" ItemPresentation="CheckBox" ItemTemplate="{StaticResource Item}"><ListBoxItem Value="a" Content="Alpha"/></ListBox>
        <ComboBox IsEditable="True" SelectionMode="Multiple" SelectionPresentation="Tokens" SelectedValues="{Binding Tags}" ItemTemplate="{StaticResource Item}"/>
        </StackPanel>"#,
    );
    let window = parse(&xml).unwrap();
    let children = window.content.children();
    let ElementKind::ListBox(list) = &children[0].kind else {
        panic!()
    };
    assert_eq!(
        list.item_presentation,
        ListItemPresentationDefinition::CheckBox
    );
    validate_item_template(list.item_template.as_ref().unwrap()).unwrap();
    let ElementKind::ComboBox(combo) = &children[1].kind else {
        panic!()
    };
    assert_eq!(
        combo.selection_presentation,
        ComboSelectionPresentationDefinition::Tokens
    );
    assert!(combo.item_template.is_some());
    assert_eq!(
        children[1].common.bindings["SelectedValues"].mode,
        BindingMode::TwoWay
    );
    let code = compile(&xml).unwrap();
    assert!(code.contains("ListItemPresentationDefinition::CheckBox"));
    assert!(code.contains("ComboSelectionPresentationDefinition::Tokens"));
}

#[test]
fn selection_modes_and_item_template_scope_reject_ambiguous_or_interactive_markup() {
    for body in [
        "<ComboBox SelectionPresentation='Tokens'/>",
        "<ComboBox SelectionMode='Multiple' Text='free'/>",
        "<ComboBox SelectionMode='Multiple' Text='{Binding Text}'/>",
        "<ComboBox SelectionMode='Multiple' SelectedValue='{Binding Key}'/>",
        "<ComboBox IsEditable='True' Text='{Binding Text}' SelectedValue='{Binding Key}'/>",
        "<ListBox ItemPresentation='CheckBox'/>",
        "<ListBox SelectionMode='Multiple' ItemPresentation='RadioButton'/>",
        "<ListBox CommitOnLostFocus='False'/>",
        "<ListBox><ListBox.ItemTemplate><Button Content='bad'/></ListBox.ItemTemplate></ListBox>",
        "<ListBox><ListBox.ItemTemplate><TextBlock x:Name='Escapes' Text='{Binding Text}'/></ListBox.ItemTemplate></ListBox>",
        "<ListBox><ListBox.ItemTemplate><TextBlock Text='{Binding ApplicationSecret}'/></ListBox.ItemTemplate></ListBox>",
        "<ListBox><ListBox.ItemTemplate><TextBlock Text='{Binding Text, Mode=TwoWay}'/></ListBox.ItemTemplate></ListBox>",
        "<ListBox><ListBox.ItemTemplate><TextBlock Text='{Binding Text, ElementName=Outside}'/></ListBox.ItemTemplate></ListBox>",
        "<ListBox><ListBox.ItemTemplate><TextBlock Text='{Binding Column32}'/></ListBox.ItemTemplate></ListBox>",
        "<ListBox><ListBox.ItemTemplate><StackPanel Width='4097'/></ListBox.ItemTemplate></ListBox>",
        "<ListBox><ListBox.ItemTemplate><StackPanel Surface='Card'/></ListBox.ItemTemplate></ListBox>",
    ] {
        assert!(parse(&source(body)).is_err(), "{body}");
    }
    assert!(parse(&source("<ComboBox IsEditable='True' Text='free text'/>")).is_ok());
    assert!(parse(&source("<ListBox><ListBox.ItemTemplate><TextBlock Text='{Binding Column31}'/></ListBox.ItemTemplate></ListBox>")).is_ok());
}

#[test]
fn lookup_search_settings_and_multiple_tokens_are_strictly_typed() {
    let xml = source(
        "<LookupEdit SelectionMode='Multiple' SelectionPresentation='Tokens' SelectedValues='{Binding Keys}' SearchColumns='0,2' SearchMode='StartsWith' SearchComparison='Ordinal'/>",
    );
    let window = parse(&xml).unwrap();
    let ElementKind::LookupEdit(lookup) = &window.content.kind else {
        panic!()
    };
    assert_eq!(lookup.search_columns, [0, 2]);
    assert_eq!(lookup.search_mode, LookupSearchModeDefinition::StartsWith);
    assert_eq!(
        lookup.search_comparison,
        LookupSearchComparisonDefinition::Ordinal
    );
    assert!(
        compile(&xml)
            .unwrap()
            .contains("search_columns: vec![0, 2]")
    );
    for invalid in [
        "<LookupEdit SearchColumns='0,0'/>",
        "<LookupEdit SearchColumns='32'/>",
        "<LookupEdit SearchColumns='1,'/>",
        "<LookupEdit SearchMode='Fuzzy'/>",
        "<LookupEdit SearchComparison='Culture'/>",
        "<LookupEdit Text='custom'/>",
        "<LookupEdit SearchColumns='1'><LookupEdit.Columns><LookupColumn Field='name' Header='Name'/></LookupEdit.Columns></LookupEdit>",
    ] {
        assert!(parse(&source(invalid)).is_err(), "{invalid}");
    }
}

#[test]
fn item_template_node_and_depth_budgets_are_independent_of_list_length() {
    let rows = "<TextBlock Text='{Binding Text}'/>".repeat(32);
    assert!(parse(&source(&format!("<ListBox><ListBox.ItemTemplate><StackPanel>{rows}</StackPanel></ListBox.ItemTemplate></ListBox>"))).is_err());
    let mut root = "<TextBlock Text='{Binding Text}'/>".to_owned();
    for _ in 0..8 {
        root = format!("<StackPanel>{root}</StackPanel>");
    }
    assert!(
        parse(&source(&format!(
            "<ListBox><ListBox.ItemTemplate>{root}</ListBox.ItemTemplate></ListBox>"
        )))
        .is_err()
    );
}
