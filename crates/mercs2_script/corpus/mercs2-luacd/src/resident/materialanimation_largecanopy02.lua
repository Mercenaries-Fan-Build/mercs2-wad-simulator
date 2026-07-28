function OnStateChange(uiGuid, uiNodeHashName, uiStateHashName)
  if uiStateHashName == String.GetHash("CollapseFireState") or uiStateHashName == String.GetHash("CollapseState") then
    Event.Create(Event.ObjectIsReady, {uiGuid}, _PlayMaterialAnims, {uiGuid})
  end
end

function _PlayMaterialAnims(uiGuid)
  Object.PlayMaterialAnimation(uiGuid, "jungle_env_largecanopy01_material_anim", false)
end
