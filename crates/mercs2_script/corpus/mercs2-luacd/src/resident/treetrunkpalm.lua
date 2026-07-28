function OnStateChange(uiGuid, uiNodeHashName, uiStateHashName)
  if uiStateHashName == String.GetHash("FireDebrisState") or uiStateHashName == String.GetHash("FireDestroyedState") then
    Event.Create(Event.ObjectIsReady, {uiGuid}, _PlayMaterialAnims, {uiGuid})
  end
end

function _PlayMaterialAnims(uiGuid)
  Object.PlayMaterialAnimation(uiGuid, "global_env_treepalm_anim", false)
  Debug.Printf(uiGuid, "im playing the palm material anim")
end
