//! ## hc_zome_membrane_invitations

use std::collections::BTreeMap;

use hdk::prelude::holo_hash::*;
use hdk::prelude::*;

use hc_zome_membrane_invitations_types::*;

entry_defs![CloneDnaRecipe::entry_def()];

#[hdk_extern]
fn init(_: ()) -> ExternResult<InitCallbackResult> {
    // grant unrestricted access to accept_cap_claim so other agents can send us claims
    let mut functions = BTreeSet::new();
    functions.insert((zome_info()?.name, "recv_remote_signal".into()));
    create_cap_grant(CapGrantEntry {
        tag: "".into(),
        // empty access converts to unrestricted
        access: ().into(),
        functions,
    })?;
    Ok(InitCallbackResult::Pass)
}

#[hdk_extern]
fn recv_remote_signal(signal: ExternIO) -> ExternResult<()> {
    let sig: Signal = signal.decode()?;
    Ok(emit_signal(&sig)?)
}

#[hdk_extern]
pub fn create_clone_dna_recipe(clone_dna_recipe: CloneDnaRecipe) -> ExternResult<EntryHashB64> {
    let hash = hash_entry(&clone_dna_recipe)?;

    create_entry(&clone_dna_recipe)?;

    create_link(
        DnaHash::from(clone_dna_recipe.original_dna_hash).retype(hash_type::Entry),
        hash.clone().into(),
        HdkLinkType::Any,
        (),
    )?;

    Ok(hash.into())
}

#[hdk_extern]
pub fn get_clone_recipes_for_dna(
    original_dna_hash: DnaHashB64,
) -> ExternResult<BTreeMap<EntryHashB64, CloneDnaRecipe>> {
    let links = get_links(
        DnaHash::from(original_dna_hash).retype(hash_type::Entry),
        None,
    )?;

    get_clone_dna_recipes(&links)
}

#[derive(Serialize, Deserialize, Debug)]
#[serde(tag = "type")]
#[serde(rename_all = "camelCase")]
pub enum Signal {
    NewInvitation {
        invitation_header_hash: HeaderHashB64,
        invitation: JoinMembraneInvitation,
    },
}

#[hdk_extern]
pub fn invite_to_join_membrane(input: InviteToJoinMembraneInput) -> ExternResult<HeaderHashB64> {
    let tag: LinkTag = match input.membrane_proof.clone() {
        None => LinkTag::new(vec![]),
        Some(mp) => LinkTag::new(mp.bytes().clone()),
    };

    let clone_dna_recipe_hash = hash_entry(&input.clone_dna_recipe)?;

    let invitee_pub_key = AgentPubKey::from(input.invitee);
    let header_hash = create_link(
        invitee_pub_key.clone().into(),
        EntryHash::from(clone_dna_recipe_hash).into(),
        HdkLinkType::Any,
        tag,
    )?;

    let invitation = JoinMembraneInvitation {
        invitee: invitee_pub_key.clone().into(),
        clone_dna_recipe: input.clone_dna_recipe,
        inviter: agent_info()?.agent_initial_pubkey.into(),
        membrane_proof: input.membrane_proof,
    };

    let signal = Signal::NewInvitation {
        invitation,
        invitation_header_hash: header_hash.clone().into(),
    };

    remote_signal(ExternIO::encode(signal)?, vec![invitee_pub_key])?;

    Ok(header_hash.into())
}

#[hdk_extern]
pub fn get_my_invitations(_: ()) -> ExternResult<BTreeMap<HeaderHashB64, JoinMembraneInvitation>> {
    let agent_info = agent_info()?;

    let links = get_links(agent_info.agent_initial_pubkey.clone().into(), None)?;

    let recipes = get_clone_dna_recipes(&links)?;

    let mut my_invitations: BTreeMap<HeaderHashB64, JoinMembraneInvitation> = BTreeMap::new();

    for link in links {
        if let Some(recipe) = recipes.get(&EntryHashB64::from(EntryHash::from(link.target))) {
            let membrane_proof = match link.tag.0.len() > 0 {
                true => Some(SerializedBytes::from(UnsafeBytes::from(link.tag.0))),
                false => None,
            };

            // Remove this get when the link struct includes author
            if let Some(el) = get(link.create_link_hash.clone(), GetOptions::default())? {
                let invitation = JoinMembraneInvitation {
                    clone_dna_recipe: recipe.clone(),
                    inviter: el.header().author().clone().into(),
                    invitee: agent_info.agent_initial_pubkey.clone().into(),
                    membrane_proof,
                };

                my_invitations.insert(link.create_link_hash.into(), invitation);
            }
        }
    }

    Ok(my_invitations)
}

fn get_clone_dna_recipes(
    links: &Vec<Link>,
) -> ExternResult<BTreeMap<EntryHashB64, CloneDnaRecipe>> {
    let get_inputs = links
        .iter()
        .map(|link| GetInput::new(link.target.clone().into(), GetOptions::default()))
        .collect();

    let elements = HDK.with(|hdk| hdk.borrow().get(get_inputs))?;

    let clones: BTreeMap<EntryHashB64, CloneDnaRecipe> = elements
        .into_iter()
        .filter_map(|e| e)
        .filter_map(|el| {
            let recipe: Option<CloneDnaRecipe> = el.entry().to_app_option().unwrap_or(None);

            recipe.map(|r| (el.header().entry_hash().unwrap().clone().into(), r))
        })
        .collect();

    Ok(clones)
}

#[hdk_extern]
pub fn remove_invitation(invitation_link_hash: HeaderHashB64) -> ExternResult<()> {
    delete_link(invitation_link_hash.into())?;

    Ok(())
}