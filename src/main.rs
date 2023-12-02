use base64::engine::general_purpose;
use base64::Engine;
use io::Result;
use regex::Regex;
use rusqlite::{named_params, params, Connection};
use std::{env, fs, io};
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
             description text
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
    let args: Vec<String> = env::args().collect();
    let dp = "9898".to_string();
    let port = args.get(1).unwrap_or(&dp).as_str();
    let server = Server::http("127.0.0.1:".to_string() + port).unwrap();
    println!("http://123:123@127.0.0.1:{}", port);
    loop {
        let request = match server.recv() {
            Ok(rq) => rq,
            Err(e) => {
                println!("error: {}", e);
                break;
            }
        };
        // Check Http auth first.

        if !check_auth(&request) {
            server_non_auth_response(request).expect("To serve non auth response");
            continue;
        }
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
        if *request.method() == Method::Get
            && (request.url().ends_with(".js") || request.url().ends_with(".css"))
        {
            serve_file(request).expect("Serve file");
            continue;
        }
        match (request.method(), request.url()) {
            (Method::Get, "/") => search_page(request),
            (Method::Get, "/search") => search_page(request),
            (Method::Post, "/search") => search_page_post(request),
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

fn find_header(headers: &[Header], name: String) -> Option<Header> {
    for header in headers {
        if header.field.as_str() == name.as_str() {
            return Some(header.clone());
        }
    }
    None
}

fn check_auth(request: &Request) -> bool {
    if let Some(header) = find_header(request.headers(), "Authorization".to_string()) {
        let dp = "123:123".to_string();
        let args: Vec<String> = env::args().collect();
        let auth = args.get(2).unwrap_or(&dp).as_str();
        let encoded: String = general_purpose::STANDARD.encode(auth);
        let full_string = "Basic ".to_string() + encoded.as_str();
        if header.value == *full_string {
            return true;
        }
    }
    false
}

fn server_non_auth_response(request: Request) -> Result<()> {
    let content_type_header =
        Header::from_bytes("WWW-Authenticate", "Basic realm=\"Recipe Helper\"")
            .expect("That we didn't put any garbage in the headers");
    request.respond(
        Response::from_data("non auth".as_bytes())
            .with_header(content_type_header)
            .with_status_code(401),
    )
}

struct Ingredient {
    id: usize,
    name: String,
}

struct Recipe {
    id: usize,
    name: String,
    ingredients: Vec<Ingredient>,
    description: Option<String>,
}
fn search_page_post(mut request: Request) -> Result<()> {
    let mut content = String::new();
    request.as_reader().read_to_string(&mut content).unwrap();

    let params = content.split('&').collect::<Vec<&str>>();
    let mut ingredients: Vec<String> = vec![];
    for param in params {
        let parts = param.split('=').collect::<Vec<&str>>();
        let id = parts.first().unwrap();
        if *id == "ingredients" {
            let value = parts.get(1).unwrap();
            ingredients.push(value.to_string());
        }
    }
    if ingredients.is_empty() {
        return_redirect("/search".to_string(), request).unwrap();
        return Ok(());
    }

    let mut placeholder_page: String = load_page_html("src/search.html");
    let mut recipe_html = String::new();

    let recipes = get_filtered_recipes(ingredients.clone());

    for recipe in recipes {
        recipe_html += recipe.render_link().as_str();
    }

    placeholder_page = placeholder_page.replace(
        "{ingredients}",
        ingredients_select_html_by_ing(Some(ingredients)).as_str(),
    );

    placeholder_page = placeholder_page.replace("*PLACEHOLDER*", recipe_html.as_str());

    return serve_bytes(
        request,
        placeholder_page.as_bytes(),
        "text/html; charset=utf-8",
    );
}

fn load_page_html(filename: &str) -> String {
    let filepath: String = filename.to_string();
    let file: String = fs::read_to_string(filepath.as_str()).unwrap();
    let mut page: String = fs::read_to_string("src/page.html").unwrap();
    page = page.replace("{body}", file.as_str());
    page
}

fn search_page(request: Request) -> Result<()> {
    let mut placeholder_page: String = load_page_html("src/search.html");
    let mut recipe_html = String::new();
    for recipe in get_recipes() {
        recipe_html += recipe.render_link().as_str();
    }

    placeholder_page =
        placeholder_page.replace("{ingredients}", ingredients_select_html(None).as_str());

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
    Some(id_cast.unwrap())
}

fn repeat_vars(count: usize) -> String {
    assert_ne!(count, 0);
    let mut s = "?,".repeat(count);
    // Remove trailing comma
    s.pop();
    s
}

fn add_missing_ingredients_to_db(list: Vec<String>) -> Vec<Ingredient> {
    let mut to_create = vec![];
    let mut existing_ids = vec![];
    for item in list.clone() {
        if let Some(number) = get_usize(item.clone()) {
            existing_ids.push(number);
        } else {
            to_create.push(item.clone());
        }
    }

    let con = get_con();
    if !to_create.is_empty() {
        let mut filter = "\"".to_owned();
        filter += to_create.join(", ").as_str();
        filter += "\"";

        let vars = repeat_vars(to_create.len());

        let sql = format!("SELECT id, name FROM ingredients WHERE name IN ({})", vars,);
        let mut stmt = con.prepare(&sql).unwrap();
        let existing_by_name: Vec<Ingredient> = stmt
            .query_map(rusqlite::params_from_iter(to_create.clone()), |row| {
                Ok(Ingredient {
                    id: row.get(0).unwrap(),
                    name: row.get(1).unwrap(),
                })
            })
            .unwrap()
            .map(|x| x.unwrap())
            .collect();
        for item in existing_by_name {
            existing_ids.push(item.id);
            to_create.retain(|x| x.clone() != item.name);
        }
    }

    for name in to_create.clone() {
        con.execute("INSERT INTO ingredients (name) VALUES (?1)", params![name])
            .expect("To add ingredient db");
        let mut stmt = con
            .prepare("SELECT id from ingredients where name = ?1;")
            .unwrap();
        let new_id = stmt
            .query_row(params![name], |row| {
                let ii: usize = row.get(0).unwrap();
                Ok(ii)
            })
            .unwrap();
        existing_ids.push(new_id);
    }
    let mut strs = vec![];
    for existing_id in existing_ids.clone() {
        strs.push(existing_id.to_string())
    }

    let vars = repeat_vars(strs.len());
    let sql = format!("SELECT id, name FROM ingredients WHERE id IN ({})", vars);
    let mut stmt = con.prepare(&sql).unwrap();

    let res = stmt
        .query_map(rusqlite::params_from_iter(strs.clone()), |row| {
            let tttt = Ingredient {
                id: row.get(0).unwrap(),
                name: row.get(1).unwrap(),
            };
            Ok(tttt)
        })
        .unwrap();
    let mut output = vec![];
    for ii in res {
        output.push(ii.unwrap());
    }
    println!("output len: {}", output.len());
    println!("existing len: {}", existing_ids.len());
    println!("strs len: {}", strs.len());
    println!("strs: {}", strs.join(","));
    println!("inputs len: {}", list.len());
    assert_eq!(output.len(), list.len());
    output
}

fn add_page_post(mut request: Request, recipe: Option<Recipe>) -> Result<()> {
    let mut content = String::new();
    request.as_reader().read_to_string(&mut content).unwrap();

    let params = content.split('&').collect::<Vec<&str>>();
    let mut name: Option<String> = None;
    let mut ingredients: Vec<String> = vec![];
    let mut description = None;
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
            "description" => {
                let decoded_desc = urlencoding::decode(value)
                    .expect("UTF-8")
                    .to_string()
                    .replace('+', " ");
                description = Some(decoded_desc.to_string());
            }
            _ => {}
        }
    }

    let mut ingredients_list = vec![];
    if !ingredients.is_empty() {
        ingredients_list = add_missing_ingredients_to_db(ingredients);
    }
    match recipe {
        None => {
            let created = Recipe::create(name.unwrap(), ingredients_list, description);
            //recipe_page(created, request)
            return_redirect(format!("/recipe/{}", created.id), request)
        }
        Some(mut recipe_object) => {
            recipe_object.ingredients = ingredients_list;
            recipe_object.name = name.unwrap();
            recipe_object.description = description;
            recipe_object.save();
            //recipe_page(recipe_object, request)
            return_redirect(format!("/recipe/{}", recipe_object.id), request)
        }
    }
}

fn add_page(request: Request, recipe: Option<Recipe>) -> Result<()> {
    let mut placeholder_page: String = load_page_html("src/add.html");
    let mut name_replace = "".to_string();
    let mut ingredients_replace = ingredients_select_html(None);
    let mut id = 0;
    let mut description_replace = "".to_string();
    if let Some(recipe_onject) = recipe {
        id = recipe_onject.id;
        name_replace = recipe_onject.name.to_string();
        ingredients_replace = ingredients_select_html(Some(&recipe_onject));
        let action = "action=\"/edit/".to_string() + recipe_onject.id.to_string().as_str() + "\"";
        placeholder_page = placeholder_page.replace("action=\"/add\"", action.as_str());
        if recipe_onject.description.is_some() {
            description_replace = recipe_onject.description.unwrap();
        }
    }
    placeholder_page = placeholder_page.replace("{id}", id.to_string().as_str());
    placeholder_page = placeholder_page.replace("{name}", name_replace.as_str());
    placeholder_page = placeholder_page.replace("{ingredients}", ingredients_replace.as_str());
    placeholder_page = placeholder_page.replace("{description}", description_replace.as_str());

    serve_bytes(
        request,
        placeholder_page.as_bytes(),
        "text/html; charset=utf-8",
    )
}

fn serve_file(request: Request) -> Result<()> {
    let filename = request
        .url()
        .split('/')
        .last()
        .expect("Request URL to have a filename");
    let filepath: String = "src/".to_string() + filename;
    let bytes = &fs::read(filepath.as_str()).unwrap();
    serve_bytes(request, bytes, "charset=utf-8")
}

// Returns an array of bytes.
fn serve_bytes(request: Request, bytes: &[u8], content_type: &str) -> Result<()> {
    let content_type_header = Header::from_bytes("Content-Type", content_type)
        .expect("That we didn't put any garbage in the headers");
    request.respond(Response::from_data(bytes).with_header(content_type_header))
}

struct RecipeShort {
    id: usize,
    name: String,
}

struct RecipeResult {
    recipe: RecipeShort,
    match_percentage: u8,
}

impl RecipeResult {
    fn render_link(self) -> String {
        let mut link = self.recipe.render_link();
        let perc = format!(" ({}% match)", self.match_percentage);
        link = link.replace("</div>", perc.as_str());
        link += "</div>";
        link
    }
}

fn get_recipes() -> Vec<RecipeShort> {
    let conn = get_con();
    let mut stmt = conn.prepare("SELECT id, name from recipes;").unwrap();

    let recipes = stmt
        .query_map((), |row| {
            Ok(RecipeShort {
                id: row.get(0).unwrap(),
                name: row.get(1).unwrap(),
            })
        })
        .unwrap();

    let mut output = vec![];
    for recipe in recipes {
        output.push(recipe.unwrap());
    }
    output
}

fn get_filtered_recipes(ingredients: Vec<String>) -> Vec<RecipeResult> {
    let con = get_con();
    let mut filter = "\"".to_owned();
    filter += ingredients.join(", ").as_str();
    filter += "\"";
    let vars = repeat_vars(ingredients.len());

    let sql = format!(
        "SELECT DISTINCT r.id, r.name from recipes as r
    join recipe_ingredients as ri on ri.recipe_id = r.id
    where ri.ingredient_id in ({})",
        vars,
    );
    let mut stmt = con.prepare(&sql).unwrap();

    let mut recipes: Vec<RecipeResult> = stmt
        .query_map(rusqlite::params_from_iter(ingredients.clone()), |row| {
            let rs = RecipeShort {
                id: row.get(0).unwrap(),
                name: row.get(1).unwrap(),
            };
            let full = get_recipe_by_id(rs.id).unwrap();
            let all_ing_cnt = full.ingredients.len();
            let mut match_cnt = 0;
            for f_ingredient in full.ingredients {
                if ingredients.contains(&f_ingredient.id.to_string()) {
                    match_cnt += 1;
                }
            }

            let perc: f32 = (match_cnt as f32 / all_ing_cnt as f32) * 100.0;
            let perc: u8 = perc.round() as u8;

            Ok(RecipeResult {
                recipe: rs,
                match_percentage: perc,
            })
        })
        .unwrap()
        .map(|x| x.unwrap())
        .collect();
    recipes.sort_by(|a, b| b.match_percentage.partial_cmp(&a.match_percentage).unwrap());
    recipes
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

fn get_all_ingredients() -> Vec<Ingredient> {
    let con = get_con();
    let mut stmt = con.prepare("SELECT id, name from ingredients;").unwrap();
    return stmt
        .query_map([], |row| {
            Ok(Ingredient {
                id: row.get(0).unwrap(),
                name: row.get(1).unwrap(),
            })
        })
        .unwrap()
        .map(|x| x.unwrap())
        .collect();
}

fn ingredients_select_html_by_ing(ingredients: Option<Vec<String>>) -> String {
    let mut html = "".to_string();
    for i in get_all_ingredients().iter() {
        if ingredients.is_some() && ingredients.clone().unwrap().contains(&i.id.to_string()) {
            html += format!(
                "<option value=\"{}\" selected=\"selected\">{}</option>",
                i.id, i.name
            )
            .as_str();
        } else {
            html += format!("<option value=\"{}\">{}</option>", i.id, i.name).as_str();
        }
    }
    html
}

fn ingredients_select_html(recipe: Option<&Recipe>) -> String {
    match recipe {
        None => ingredients_select_html_by_ing(None),
        Some(rec) => {
            let mut ings = vec![];
            for i in rec.ingredients.iter() {
                ings.push(i.id.to_string());
            }
            ingredients_select_html_by_ing(Some(ings))
        }
    }
}

impl Recipe {
    fn render(self) -> String {
        let mut placeholder: String = fs::read_to_string("src/recipe-body.html")
            .unwrap()
            .parse()
            .unwrap();
        placeholder = placeholder.replace("{name}", self.name.as_str());
        let ingredients = self
            .ingredients
            .iter()
            .by_ref()
            .map(|i| format!("<li>{}</li>", i.name.clone()))
            .collect::<Vec<String>>()
            .join("");
        placeholder = placeholder.replace("{ingredients}", ingredients.as_str());
        let mut description_text = "".to_string();
        if self.description.is_some() {
            description_text = self.description.unwrap();
            // Check for links.
            let re = Regex::new(r#"(?<link>(http.*?\s)|(http.*?$))"#).unwrap();
            description_text = re
                .replace_all(
                    description_text.as_str(),
                    "<a target=\"_blank\" href=\"$link\">$link</a>",
                )
                .to_string();
            description_text = description_text.replace(" </a>", "</a> ");
        }
        placeholder = placeholder.replace("{description}", description_text.as_str());
        placeholder
    }
    fn create(name: String, ingredients: Vec<Ingredient>, description: Option<String>) -> Recipe {
        let con = get_con();
        let description_str = description.clone().unwrap_or("".to_string());
        con.execute(
            "INSERT INTO recipes (name, description) VALUES (?1, ?2)",
            params![name, description_str],
        )
        .expect("DUPLICATE RECIPE NAME");
        let res: u32 = con
            .query_row("SELECT id FROM recipes WHERE name = (?1)", [&name], |row| {
                row.get(0)
            })
            .unwrap();

        for i in ingredients.iter().as_ref() {
            con.execute(
                "INSERT INTO recipe_ingredients (recipe_id, ingredient_id)
                 VALUES (:recipe_id, :ingredient_id)",
                named_params! {
                    ":recipe_id": res,
                    ":ingredient_id": i.id,
                },
            )
            .unwrap();
        }

        Recipe {
            id: res as usize,
            name,
            ingredients,
            description,
        }
    }
    fn save(&self) {
        let con = get_con();
        let id = self.id;
        let description = self.description.clone().unwrap_or("".to_string());
        let existing_recipe = get_recipe_by_id(id).unwrap();
        let existing_ings = existing_recipe
            .ingredients
            .iter()
            .map(|x| x.id)
            .collect::<Vec<usize>>();
        let mut to_delete = existing_ings.clone();
        for i in self.ingredients.iter() {
            if existing_ings.contains(&i.id) {
                let index = to_delete.iter().position(|x| *x == i.id).unwrap();
                to_delete.remove(index);
                continue;
            }
            con.execute(
                "INSERT INTO recipe_ingredients (recipe_id, ingredient_id)
                 VALUES (:recipe_id, :ingredient_id)",
                named_params! {
                    ":recipe_id": id,
                    ":ingredient_id": i.id,
                },
            )
            .unwrap();
        }
        // Check if anything has to be deleted.
        for ing_id in to_delete {
            con.execute(
                "DELETE FROM recipe_ingredients WHERE recipe_id = :recipe_id and ingredient_id = :ingredient_id",
                named_params! {
                    ":recipe_id": id,
                    ":ingredient_id": ing_id,
                },
            )
                .unwrap();
        }
        con.execute(
            "UPDATE recipes SET name = :name, description = :description WHERE id = :id",
            named_params! {
                ":id": id,
                ":description": description,
                ":name": self.name,
            },
        )
        .unwrap();
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
        .prepare(
            "SELECT r.id, r.name, r.description from recipes as r
where r.id = ?1
        ;",
        )
        .unwrap();

    let tt = stmt.query_row(params![id], |row| {
        let mut desc: Option<String> = None;
        if let Ok(text) = row.get(2) {
            desc = Some(text);
        }
        let recipe = Recipe {
            id: row.get(0).unwrap(),
            name: row.get(1).unwrap(),
            ingredients: vec![],
            description: desc,
        };
        Ok(recipe)
    });

    if tt.is_err() {
        return None;
    }
    let mut recipe = tt.unwrap();

    let mut stmt = conn
        .prepare("SELECT ig.id, ig.name from recipe_ingredients as i join ingredients as ig on ig.id=i.ingredient_id where i.recipe_id = :id;")
        .unwrap();
    let ing = stmt.query_map(params![id], |row| {
        Ok(Ingredient {
            id: row.get(0).unwrap(),
            name: row.get(1).unwrap(),
        })
    });

    for io in ing.unwrap().flatten().collect::<Vec<Ingredient>>() {
        recipe.ingredients.push(io);
    }
    Some(recipe)
}

fn id_from_request(request: &Request) -> Option<usize> {
    let url = request.url();
    if let Some(id) = url.split('/').collect::<Vec<&str>>().last() {
        return get_usize(id.to_string());
    }
    None
}

fn recipe_page(recipe: Recipe, request: Request) -> Result<()> {
    let mut placeholder_page: String = load_page_html("src/recipe.html");
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
    let _ = request.respond(response);
    Ok(())
}
