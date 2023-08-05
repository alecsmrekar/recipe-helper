use io::Result;
use rusqlite::{params, Connection};
use std::{fs, io};
use tiny_http::{Header, Method, Request, Response, Server};

fn main() {
    serve();
}

fn get_con() -> Connection {
    Connection::open("main.db").expect("To open an SQLite connection")
}

fn serve() {
    let conn = get_con();
    conn.execute(
        "create table if not exists recipes (
             id integer primary key,
             name text not null unique,
             ingredients text
         )",
        (),
    )
    .expect("To create a table");

    let server = Server::http("127.0.0.1:9898").unwrap();
    println!("http://127.0.0.1:9898");
    loop {
        let request = match server.recv() {
            Ok(rq) => rq,
            Err(e) => {
                println!("error: {}", e);
                break;
            }
        };
        if *request.method() == Method::Get && request.url().starts_with("/recipe/") {
            recipe_page(request).expect("That recipe page is ok");
            continue;
        }
        match (request.method(), request.url()) {
            (Method::Get, "/") => landing_page(request),
            (Method::Get, "/search") => search_page(request),
            (Method::Get, "/add") => add_page(request),
            (Method::Post, "/add") => add_page_post(request),
            _ => serve_bytes(
                request,
                "Hello, world!".as_bytes(),
                "text/html; charset=utf-8",
            ),
        }
        .unwrap();
        println!("received!");
    }
}

type Ingredient = String;

struct Recipe {
    id: usize,
    name: String,
    ingredients: Vec<Ingredient>,
}

fn landing_page(request: Request) -> Result<()> {
    serve_bytes(
        request,
        include_bytes!("landing.html"),
        "text/html; charset=utf-8",
    )
}

fn search_page(request: Request) -> Result<()> {
    let mut placeholder_page: String = fs::read_to_string("src/search.html")
        .unwrap()
        .parse()
        .unwrap();
    let mut recipe_html = String::new();
    for recipe in get_recipes() {
        recipe_html += recipe.render_link().as_str();
    }

    placeholder_page = placeholder_page.replace("*PLACEHOLDER*", recipe_html.as_str());

    return serve_bytes(
        request,
        placeholder_page.as_bytes(),
        "text/html; charset=utf-8",
    );
}

fn add_page_post(mut request: Request) -> Result<()> {
    let mut content = String::new();
    request.as_reader().read_to_string(&mut content).unwrap();

    let params = content.split('&').collect::<Vec<&str>>();
    let mut name: Option<String> = None;
    let mut ingredients: Option<String>;
    for param in params {
        let parts = param.split('=').collect::<Vec<&str>>();
        let id = parts.first().unwrap();
        let value = parts.get(1).unwrap();
        match *id {
            "name" => {
                name = Some(value.to_string());
            }
            "ingredients" => {
                ingredients = Some(value.to_string());
            }
            _ => {}
        }
    }

    //if !name.is_some() {
    // return 500;
    //}

    let ingredients_list: Vec<Ingredient> = vec![];
    //if ingredients.is_some() {
    //ingredients = ingredients;
    // split by new line
    //}

    let _ = Recipe::create(name.unwrap(), ingredients_list);

    serve_bytes(
        request,
        include_bytes!("add.html"),
        "text/html; charset=utf-8",
    )
}

fn add_page(request: Request) -> Result<()> {
    serve_bytes(
        request,
        include_bytes!("add.html"),
        "text/html; charset=utf-8",
    )
}

// Returns an array of bytes.
fn serve_bytes(request: Request, bytes: &[u8], content_type: &str) -> Result<()> {
    let content_type_header = Header::from_bytes("Content-Type", content_type)
        .expect("That we didn't put any garbage in the headers");
    request.respond(Response::from_data(bytes).with_header(content_type_header))
}

fn get_recipes() -> Vec<Recipe> {
    let mut recipes = vec![];
    let r1 = Recipe {
        id: 1,
        name: "Pizza".to_string(),
        ingredients: vec!["Flour".to_string(), "Kapre".to_string()],
    };
    let r2 = Recipe {
        id: 2,
        name: "Pasta".to_string(),
        ingredients: vec!["Flour".to_string(), "Tomato".to_string()],
    };
    let r3 = Recipe {
        id: 3,
        name: "Kuskus zelo dober".to_string(),
        ingredients: vec!["Kuskus".to_string(), "Suhi paradajzi".to_string()],
    };
    recipes.push(r1);
    recipes.push(r2);
    recipes.push(r3);
    recipes
}

impl Recipe {
    fn render_link(self) -> String {
        let mut html = "<div>".to_string();
        html += self.name.as_str();
        let link =
            " (<a href=\"/recipe/".to_owned() + self.id.to_string().as_str() + "\">more</a>)";
        html += link.as_str();
        html += "</div>";
        html.to_string()
    }
    fn render(self) -> String {
        let mut html = "<div>".to_string();
        html += self.name.as_str();
        html += "</div>";
        let ingredients = self
            .ingredients
            .into_iter()
            .collect::<Vec<String>>()
            .join(", ");
        html = html + "Ingredients: " + ingredients.as_str();
        html += "</div>";
        html.to_string()
    }
    fn create(name: String, ingredients: Vec<Ingredient>) -> Recipe {
        let con = get_con();
        con.execute(
            "INSERT INTO recipes (name, ingredients) VALUES (?1, ?2)",
            (&name, &ingredients.join(",")),
        )
        .expect("To write to db");
        let res: u32 = con
            .query_row("SELECT id FROM recipes WHERE name = (?1)", [&name], |row| {
                row.get(0)
            })
            .unwrap();
        Recipe {
            id: res as usize,
            name,
            ingredients,
        }
    }
}

fn get_recipe_by_id(id: usize) -> Option<Recipe> {
    get_recipes().into_iter().find(|recipe| recipe.id == id)
}

fn recipe_page(request: Request) -> Result<()> {
    let url = request.url();
    let id = *url.split('/').collect::<Vec<&str>>().last().unwrap();
    let id_cast: usize = id.parse().unwrap();
    let recipe = get_recipe_by_id(id_cast).unwrap();

    let mut placeholder_page: String = fs::read_to_string("src/recipe.html")
        .unwrap()
        .parse()
        .unwrap();
    placeholder_page = placeholder_page.replace("*PLACEHOLDER*", recipe.render().as_str());
    return serve_bytes(
        request,
        placeholder_page.as_bytes(),
        "text/html; charset=utf-8",
    );
}
