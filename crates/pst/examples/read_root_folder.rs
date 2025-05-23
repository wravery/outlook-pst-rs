use clap::Parser;
use outlook_pst::{messaging::store::UnicodeStore, *};
use std::rc::Rc;

mod args;

fn main() -> anyhow::Result<()> {
    let args = args::Args::try_parse()?;
    let pst = UnicodePstFile::open(&args.file).unwrap();
    let store = UnicodeStore::read(Rc::new(pst)).unwrap();
    let hierarchy_table = store.root_hierarchy_table()?;
    let context = hierarchy_table.context();

    for row in hierarchy_table.rows_matrix() {
        println!("Row: 0x{:X}", u32::from(row.id()));
        println!("Version: 0x{:X}", row.unique());

        for (column, value) in context.columns().iter().zip(row.columns(context)?) {
            println!(
                " Column: Property ID: 0x{:04X}, Type: {:?}",
                column.prop_id(),
                column.prop_type()
            );

            let Some(value) = value else {
                println!("  Value: None");
                continue;
            };

            println!("  Record: {value:?}");

            let value = store.read_table_column(&hierarchy_table, &value, column.prop_type())?;
            println!("  Value: {:?}", value);
        }
    }

    Ok(())
}
