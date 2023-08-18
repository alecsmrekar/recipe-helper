use io::Result;
use rusqlite::{named_params, Connection, params};
use std::{fs, io};
use std::rc::Rc;
use rusqlite::types::Value;
use tiny_http::{Header, Method, Request, Response, Server, StatusCode};

fn main() {
    serve();
}

fn get_con() -> Connection {
    Connection::open("main.db").expect("To open an SQLite connection")
}

fn all_ingredients() -> Vec<Ingredient> {
    vec![]
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

    conn.execute(
        "create table if not exists ingredients (
             id integer primary key,
             name text not null unique
        )",
        (),
    )
        .expect("To create a table");

    conn.execute(
        "create table if not exists recipe_ingredients (
        recipe_id integer not null references recipes(id),
        ingredient_id  integer not null references ingredients(id),
        primary key (recipe_id, ingredient_id)
      )",
        (),
    )
        .expect("To create a table");

    // https://stackoverflow.com/a/8003151.
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
            let id = id_from_request(&request);
            if id.is_none() {
                return_redirect("/".to_string(), request).unwrap();
                continue;
            }
            let recipe = get_recipe_by_id(id.unwrap());
            recipe.unwrap().delete();
            return_redirect("/search".to_string(), request).unwrap();
            continue;
        }
        if *request.method() == Method::Get && request.url().starts_with("/edit/") {
            let id = id_from_request(&request);
            if id.is_none() {
                return_redirect("/".to_string(), request).unwrap();
                continue;
            }
            let recipe = get_recipe_by_id(id.unwrap());
            add_page(request, recipe).expect("Add page");
            continue;
        }
        if *request.method() == Method::Post && request.url().starts_with("/edit/") {
            let id = id_from_request(&request);
            if id.is_none() {
                return_redirect("/".to_string(), request).unwrap();
                continue;
            }
            let recipe = get_recipe_by_id(id.unwrap());
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

struct Ingredient {
    id: usize,
    name: String
}

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

fn get_usize(text: String) -> Option<usize> {
    let id_cast = text.parse::<usize>();
    if id_cast.is_err() {
        return None;
    }
    return Some(id_cast.unwrap());
}

fn add_missing_ingredients_to_db(list: Vec<String>) -> Vec<Ingredient> {
    let mut to_create = vec![];
    let mut existing_ids = vec![];
    for item in list {
        if let Some(number) = get_usize(item) {
            existing_ids.push(number);
        }
        else {
            to_create.push(item.clone());
        }
    }

    let con = get_con();
    for name in to_create {
        con.execute(
            "INSERT INTO ingredients (name) VALUES (?1)",
            params![name],
        )
            .expect("To add ingredient db");
        let mut stmt = con
            .prepare("SELECT id from ingredients where name = ?1;")
            .unwrap();
        let new_id = stmt.query_row(params![name], |row| {
            let ii: usize = row.get(0).unwrap();
            return Ok(ii);
        }).unwrap();
        existing_ids.push(new_id);
    }

    let mut stmt = con
        .prepare("SELECT id, name from ingredients where id IN ?1;")
        .unwrap();
    let mut ints = vec![];
    for existing_id in existing_ids {
        ints.push( Value::from(existing_id as i64));
    }
    let test = Rc::new(ints);
    let test2 = Rc::new(ints.iter().as_ref().iter().map(Value::from).collect::<Vec<Value>>());
    let ingredients = stmt
        .query_map(params![test2], |row| {
            Ok(Ingredient {
                id: row.get(0).unwrap(),
                name: row.get(1).unwrap()
            })
        }).unwrap().map(|x| x.unwrap()).collect();

    return ingredients;
}

fn add_page_post(mut request: Request, recipe: Option<Recipe>) -> Result<()> {
    let mut content = String::new();
    request.as_reader().read_to_string(&mut content).unwrap();

    let params = content.split('&').collect::<Vec<&str>>();
    let mut name: Option<String> = None;
    let mut ingredients: Vec<String> = vec![];
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
                ingredients.push(value.to_string());
            }
            _ => {}
        }
    }

    let a=2;
    //if !name.is_some() {
    // return 500;
    //}

    let mut ingredients_list = add_missing_ingredients_to_db(ingredients);
    /*if let Some(ingredients_object) = ingredients {
        let decoded = urlencoding::decode(ingredients_object.as_str())
            .expect("UTF-8")
            .to_string();
        ingredients_list = decoded
            .lines()
            .collect::<Vec<&str>>()
            .iter()
            .map(|x| x.to_string() as Ingredient)
            .collect::<Vec<Ingredient>>();
    }*/

    match recipe {
        None => {
            let created = Recipe::create(name.unwrap(), ingredients_list);
            //recipe_page(created, request)
            return_redirect(format!("/recipe/{}", created.id), request)
        }
        Some(mut recipe_object) => {
            recipe_object.ingredients = ingredients_list;
            recipe_object.name = name.unwrap();
            recipe_object.save();
            //recipe_page(recipe_object, request)
            return_redirect(format!("/recipe/{}", recipe_object.id), request)
        }
    }
}

fn add_page(request: Request, recipe: Option<Recipe>) -> Result<()> {
    let mut placeholder_page: String = fs::read_to_string("src/add.html").unwrap().parse().unwrap();
    let mut name_replace = "".to_string();
    let mut ingredients_replace = ingredients_select_html();
    let mut id = 0;
    if let Some(recipe_onject) = recipe {
        id = recipe_onject.id;
        name_replace = recipe_onject.name.to_string();
        ingredients_replace = todo!();
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

struct RecipeShort{
    id: usize,
    name: String,
}

fn get_recipes() -> Vec<RecipeShort> {
    let conn = get_con();
    let mut stmt = conn
        .prepare("SELECT id, name from recipes;")
        .unwrap();

    let recipes = stmt
        .query_map((), |row| {
            Ok(RecipeShort {
                id: row.get(0).unwrap(),
                name: row.get(1).unwrap()
            })
        })
        .unwrap();

    let mut output = vec![];
    for recipe in recipes {
        output.push(recipe.unwrap());
    }
    output
}

impl RecipeShort {
    fn render_link(self) -> String {
        let mut html = "<div>".to_string();
        html += self.name.as_str();
        let link =
            " (<a href=\"/recipe/".to_owned() + self.id.to_string().as_str() + "\">more</a>)";
        html += link.as_str();
        html += "</div>";
        html.to_string()
    }
}

fn ingredients_select_html() -> String {
    return "<option value=\"1\">white</option>".to_string();
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
            .iter().by_ref().map(|i| i.name.clone())
            .collect::<Vec<String>>()
            .join(", ");
        html = html + "Ingredients: " + ingredients.as_str();
        html += "</div>";
        html.to_string()
    }
    fn ingredients_string(&self) -> String {
        return "a".to_string();
    }
    fn create(name: String, ingredients: Vec<Ingredient>) -> Recipe {
        let con = get_con();
        con.execute(
            "INSERT INTO recipes (name) VALUES (?1)",
            params![name],
        )
        .expect("To write to db");
        let res: u32 = con
            .query_row("SELECT id FROM recipes WHERE name = (?1)", [&name], |row| {
                row.get(0)
            })
            .unwrap();

        for i in ingredients.iter().as_ref() {
            con.execute(
                "INSERT INTO recipe_ingredients (recipe_id, ingredient_id) VALUES (?1 ?2)",
                params![res, i.id],
            )
                .expect("To write to db");
        }

        Recipe {
            id: res as usize,
            name,
            ingredients,
        }
    }
    fn save(&self) {
        let name = self.name.as_str();
        let id = self.id;
        !todo!()
    }
    fn delete(self) {
        let con = get_con();
        let mut stmt = con.prepare("DELETE FROM recipes WHERE id = :id").unwrap();
        stmt.execute(named_params! { ":id": self.id })
            .expect("To delete recipe");
    }
}

fn get_recipe_by_id(id: usize) -> Option<Recipe> {
    let conn = get_con();
    let mut stmt = conn
        .prepare("SELECT r.id, r.name from recipes as r
where r.id = ?1
        ;")
        .unwrap();

    let tt = stmt.query_row(params![id], |row| {
            let recipe = Recipe {
                id: row.get(0).unwrap(),
                name: row.get(1).unwrap(),
                ingredients: vec![],
            };
            return Ok(recipe);
        });

    if tt.is_err() {
        return None;
    }
    let mut recipe = tt.unwrap();

    let mut stmt = conn
        .prepare("SELECT ig.id, ig.name from recipe_ingredients as i join ingredients as ig on ig.id=i.ingredient_id where i.recipe_id = :id;")
        .unwrap();
    let ing = stmt.query_map(params![id], |row| {
        return Ok(Ingredient{
            id: row.get(0).unwrap(),
            name: row.get(1).unwrap()
        });
    });

    for io in ing.unwrap() {
        if let Ok(rr) = io {
            recipe.ingredients.push(rr);
        }
    }
    return Some(recipe);
}

fn id_from_request(request: &Request) -> Option<usize> {
    let url = request.url();
    if let Some(id) = url.split('/').collect::<Vec<&str>>().last() {
        let ss = id.to_string();
        return get_usize(id.to_string());
    }
    None
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
    let id = id_from_request(&request);
    if id.is_none() {
        return_redirect("/".to_string(), request).unwrap();
        return Ok(());
    }
    let recipe = get_recipe_by_id(id.unwrap()).unwrap();
    recipe_page(recipe, request)
}

fn return_redirect(destination: String, request: Request) -> Result<()> {
    let header = Header::from_bytes("Location", destination)
        .expect("That we didn't put any garbage in the headers");

    let response = Response::from_data(vec![])
        .with_status_code(301)
        .with_header(header);
    request.respond(response);
    Ok(())
}