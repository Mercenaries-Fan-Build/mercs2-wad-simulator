inherit("OrientedBlippable")
inherit("MrxFactionManager")
sTexture = "temp_radar_icon_airplane"
nSize = 5
tColorAlly = {
  0,
  127,
  255
}
tColorNeutral = {
  200,
  200,
  200
}
tColorEnemy = {
  255,
  0,
  0
}
tColorPmc = {
  0,
  255,
  0
}

function OnActivate(uGuid, uRuntimeOwner, iArg)
  local oPrototype = getfenv()
  local oInstance = oPrototype:Create(uGuid, uRuntimeOwner)
  local sFaction = MrxFactionManager.GetFaction(uGuid)
  if sFaction then
    nRelation = Ai.GetRelation(Pg.GetGuidByName(sFaction), Pg.GetGuidByName("PMC"))
  else
    nRelation = 0
  end
  if Object.HasLabel(uGuid, "PMC") then
    Object.SetUnkillable(uGuid, true, "Support")
  end
  if Object.HasLabel(uGuid, "pmc") then
    oInstance.tColor = tColorPmc
  elseif nRelation < 60 and nRelation > -60 then
    oInstance.tColor = tColorNeutral
  elseif nRelation <= -60 then
    oInstance.tColor = tColorEnemy
  elseif nRelation >= 60 then
    oInstance.tColor = tColorAlly
  end
  if Object.HasLabel(uGuid, "C130") then
    oInstance.sTexture = "temp_radar_icon_c130"
  elseif Object.HasLabel(uGuid, "Mig27") then
    oInstance.sTexture = "temp_radar_icon_mig27"
  elseif Object.HasLabel(uGuid, "F35") then
    oInstance.sTexture = "temp_radar_icon_f35"
  elseif Object.HasLabel(uGuid, "b2") then
    oInstance.sTexture = "temp_radar_icon_b2"
  elseif Object.HasLabel(uGuid, "f117") then
    oInstance.sTexture = "temp_radar_icon_f117"
  elseif Object.HasLabel(uGuid, "a10") then
    oInstance.sTexture = "temp_radar_icon_a10"
  elseif Object.HasLabel(uGuid, "ov10") then
    oInstance.sTexture = "temp_radar_icon_ov10"
  elseif Object.HasLabel(uGuid, "cruisemissile") then
    oInstance.sTexture = "temp_radar_icon_cruisemissile"
  end
  OrientedBlippable.SetBlipped(oInstance)
  oInstance:SetBlipped()
end
