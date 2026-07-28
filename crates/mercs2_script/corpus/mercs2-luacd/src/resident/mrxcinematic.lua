import("MrxUtil")

function PlaceholderSequence(tSlides, fCallback, tCallbackArgs)
  local nSlides = table.getn(tSlides)
  tSlides[nSlides].fCallback = fCallback
  tSlides[nSlides].tCallbackData = tCallbackArgs
  tSlides[nSlides].nFadeInTime = 0
  for i = nSlides - 1, 1, -1 do
    tSlides[i].fCallback = _DisplaySlide
    tSlides[i].tCallbackData = {
      tSlides[i + 1]
    }
    if 1 < i then
      tSlides[i].nFadeInTime = 0
    end
    tSlides[i].nFadeOutTime = 0
  end
  _DisplaySlide(tSlides[1])
end

function _DisplaySlide(tSlideData)
  if not tSlideData.sTexture then
    tSlideData.sTexture = "temp_placeholder"
  end
  Hud.CinematicPlaceholder:Show(tSlideData)
end
