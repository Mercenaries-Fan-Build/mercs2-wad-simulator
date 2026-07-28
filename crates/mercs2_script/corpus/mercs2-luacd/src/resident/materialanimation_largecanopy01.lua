function OnStateChange(uiGuid, uiNodeHashName, uiStateHashName)
  if uiNodeHashName == String.GetHash("Slice00") and uiStateHashName == String.GetHash("CollapseFireState") then
    Event.Create(Event.ObjectIsReady, {uiGuid}, _PlayMaterialAnims, {uiGuid})
  end
end

function _PlayMaterialAnims(uiGuid)
  Object.PlayMaterialAnimation(uiGuid, "jungle_env_largecanopy01_material_anim", false)
end
