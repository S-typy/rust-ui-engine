//! Explicit-path DOCX profile inspection and round-trip utility.
use rust_desktop_ui_controls::rich_document::{
    Paragraph, RichBlock, RichDocument, RichTable, RunStyle,
};
use std::path::Path;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<_> = std::env::args().collect();
    match args.get(1).map(String::as_str){
        Some("sample") if args.len()==3=>{
            let mut document=RichDocument::from_text("Rich document\nРусский текст, 日本語, العربية and emoji 🦀").unwrap();
            let RichBlock::Paragraph(p)=&mut document.blocks[0]else{unreachable!()};p.format(0..13,RunStyle{size:24.0,bold:true,color:[0,90,180],..Default::default()})?;
            document.blocks.push(RichBlock::Table(RichTable{columns:vec![200.0,200.0],rows:vec![vec![Paragraph::text("Item"),Paragraph::text("Value")],vec![Paragraph::text("Alpha"),Paragraph::text("42")]]}));
            let export=document.to_docx()?;std::fs::write(Path::new(&args[2]),export.bytes)?;println!("Saved DOCX profile sample; {} diagnostics",export.diagnostics.len());
        }
        Some("roundtrip") if args.len()==4=>{
            let bytes=std::fs::read(Path::new(&args[2]))?;let imported=RichDocument::from_docx(&bytes)?;
            for diagnostic in &imported.diagnostics{println!("{}: {}",diagnostic.part,diagnostic.message);}
            let exported=imported.document.to_docx()?;std::fs::write(Path::new(&args[3]),&exported.bytes)?;
            let reparsed=RichDocument::from_docx(&exported.bytes)?;if reparsed.document!=imported.document{return Err("Supported document structure changed in round-trip".into());}
            println!("Round-trip verified: {} blocks; {} bytes",imported.document.blocks.len(),exported.bytes.len());
        }
        Some("inspect") if args.len()==3=>{let imported=RichDocument::from_docx(&std::fs::read(Path::new(&args[2]))?)?;println!("{}",imported.document.to_json()?);for diagnostic in imported.diagnostics{eprintln!("{}: {}",diagnostic.part,diagnostic.message);}}
        _=>return Err("Usage: rich_document_codec sample OUT.docx | roundtrip IN.docx OUT.docx | inspect IN.docx".into()),
    }
    Ok(())
}
