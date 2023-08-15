use io::Result;
use rusqlite::{named_params, Connection};
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
            recipe_page_from_request(request).expect("That recipe page is ok");
            continue;
        }
        if *request.method() == Method::Get && request.url().starts_with("/delete/") {
            let recipe = get_recipe_by_id(id_from_request(&request));
            recipe.unwrap().delete();
            search_page(request).expect("Search page");
            continue;
        }
        if *request.method() == Method::Get && request.url().starts_with("/edit/") {
            let recipe = get_recipe_by_id(id_from_request(&request));
            add_page(request, recipe).expect("Add page");
            continue;
        }
        if *request.method() == Method::Post && request.url().starts_with("/edit/") {
            let recipe = get_recipe_by_id(id_from_request(&request));
            add_page_post(request, recipe).expect("Add page POST");
            continue;
        }
        match (request.method(), request.url()) {
            (Method::Get, "/") => landing_page(request),
            (Method::Get, "/search") => search_page(request),
            (Method::Get, "/add") => add_page(request, None),
            (Method::Post, "/add") => add_page_post(request, None),
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

fn add_page_post(mut request: Request, recipe: Option<Recipe>) -> Result<()> {
    let mut content = String::new();
    request.as_reader().read_to_string(&mut content).unwrap();

    let params = content.split('&').collect::<Vec<&str>>();
    let mut name: Option<String> = None;
    let mut ingredients: Option<String> = None;
    for param in params {
        // @todo refactor.
        let parts = param.split('=').collect::<Vec<&str>>();
        let id = parts.first().unwrap();
        let value = parts.get(1).unwrap();
        match *id {
            "name" => {
                let decoded_name = urlencoding::decode(value)
                    .expect("UTF-8")
                    .to_string()
                    .replace('+', " ");
                name = Some(decoded_name);
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

    let mut ingredients_list: Vec<Ingredient> = vec![];
    if let Some(ingredients_object) = ingredients {
        let decoded = urlencoding::decode(ingredients_object.as_str())
            .expect("UTF-8")
            .to_string();
        ingredients_list = decoded
            .lines()
            .collect::<Vec<&str>>()
            .iter()
            .map(|x| x.to_string() as Ingredient)
            .collect::<Vec<Ingredient>>();
    }

    match recipe {
        None => {
            let created = Recipe::create(name.unwrap(), ingredients_list);
            recipe_page(created, request)
        }
        Some(mut recipe_object) => {
            recipe_object.ingredients = ingredients_list;
            recipe_object.name = name.unwrap();
            recipe_object.save();
            recipe_page(recipe_object, request)
        }
    }
}

fn add_page(request: Request, recipe: Option<Recipe>) -> Result<()> {
    let mut placeholder_page: String = fs::read_to_string("src/add.html").unwrap().parse().unwrap();
    let mut name_replace = "".to_string();
    let mut ingredients_replace = "".to_string();
    let mut id = 0;
    if let Some(recipe_onject) = recipe {
        id = recipe_onject.id;
        name_replace = recipe_onject.name.to_string();
        ingredients_replace = recipe_onject.ingredients_string().replace(',', "\r\n");
        placeholder_page = placeholder_page.replace(
            "action=\"/add",
            (String::from("action=\"/edit/") + id.to_string().as_str()).as_str(),
        );
    }
    placeholder_page = placeholder_page.replace("{id}", id.to_string().as_str());
    placeholder_page = placeholder_page.replace("{name}", name_replace.as_str());
    placeholder_page = placeholder_page.replace("{ingredients}", ingredients_replace.as_str());

    serve_bytes(
        request,
        placeholder_page.as_bytes(),
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
    let conn = get_con();
    let mut stmt = conn
        .prepare("SELECT id, name, ingredients from recipes;")
        .unwrap();

    let recipes = stmt
        .query_map((), |row| {
            Ok(Recipe {
                id: row.get(0).unwrap(),
                name: row.get(1).unwrap(),
                ingredients: parse_ingredients(row.get(2).unwrap()),
            })
        })
        .unwrap();

    let mut output = vec![];
    for recipe in recipes {
        output.push(recipe.unwrap());
    }
    output
}

fn parse_ingredients(input: String) -> Vec<Ingredient> {
    if input.is_empty() {
        return vec![];
    }
    let strings = input.split(',').collect::<Vec<&str>>();
    let output = strings
        .iter()
        .map(|x| x.to_string() as Ingredient)
        .collect::<Vec<Ingredient>>();
    output
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
    fn ingredients_string(&self) -> String {
        self.ingredients.join(",")
    }
    fn create(name: String, ingredients: Vec<Ingredient>) -> Recipe {
        let con = get_con();
        let ing = ingredients.join(",");
        con.execute(
            "INSERT INTO recipes (name, ingredients) VALUES (?1, ?2)",
            (&name, &ing),
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
    fn save(&self) {
        let name = self.name.as_str();
        let id = self.id;

        get_con()
            .execute(
                "UPDATE recipes SET name = ?1, ingredients = ?2 WHERE id = ?3",
                (&name, self.ingredients_string(), id),
            )
            .expect("To update the db");
    }
    fn delete(self) {
        let con = get_con();
        let mut stmt = con.prepare("DELETE FROM recipes WHERE id = :id").unwrap();
        stmt.execute(named_params! { ":id": self.id })
            .expect("To delete recipe");
    }
}

fn get_recipe_by_id(id: usize) -> Option<Recipe> {
    get_recipes().into_iter().find(|recipe| recipe.id == id)
}

fn id_from_request(request: &Request) -> usize {
    let url = request.url();
    let id = *url.split('/').collect::<Vec<&str>>().last().unwrap();
    let id_cast: usize = id.parse().unwrap();
    id_cast
}

fn recipe_page(recipe: Recipe, request: Request) -> Result<()> {
    let mut placeholder_page: String = fs::read_to_string("src/recipe.html")
        .unwrap()
        .parse()
        .unwrap();
    placeholder_page = placeholder_page.replace("{id}", recipe.id.to_string().as_str());
    placeholder_page = placeholder_page.replace("*PLACEHOLDER*", recipe.render().as_str());
    return serve_bytes(
        request,
        placeholder_page.as_bytes(),
        "text/html; charset=utf-8",
    );
}

fn recipe_page_from_request(request: Request) -> Result<()> {
    let recipe = get_recipe_by_id(id_from_request(&request)).unwrap();
    recipe_page(recipe, request)
}
